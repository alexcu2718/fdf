//! # fdf - A High-Performance Parallel File System Traversal Library
//!
//! `fdf` is a Rust library designed for efficient, parallel directory traversal
//! with extensive filtering capabilities. It leverages Rayon for parallel processing
//! and uses platform-specific optimisations for maximum performance.
//!
//! **This will be renamed before a 1.0!**
//!
//! ## Features
//!
//! - **Parallel Processing**: Utilises Rayon's work-stealing scheduler for concurrent
//!   directory traversal
//! - **Platform Optimisations**: Linux-specific `getdents` system calls for optimal
//!   performance, with fallbacks for other platforms
//! - **Flexible Filtering**: Support for multiple filtering criteria:
//!   - File name patterns (regex), glob to be added shortly(CLI only for now)
//!   - File size ranges
//!   - File types (regular, directory, symlink, etc.)
//!   - File extensions
//!   - Hidden file handling
//!   - Custom filter functions
//! - **Cycle Detection**: Automatic symlink cycle prevention using inode caching
//! - **Depth Control**: Configurable maximum search depth
//! - **Path Canonicalisation**: Optional path resolution to absolute paths
//! - **Error Handling**: Configurable error reporting with detailed diagnostics
//!
//! ## Performance Characteristics
//!
//! - Uses mimalloc as global allocator on supported platforms for improved
//!   memory allocation performance
//! - Batched result delivery to minimise channel contention
//! - Zero-copy path handling where possible
//! - Avoids unnecessary `stat` calls through careful API design
//!
//! ## Platform Support
//!
//! - **Linux**: Optimised with direct `getdents` system calls
//! - **macOS/BSD**: Standard `readdir` with potential for future `getattrlistbulk` optimisation
//! - **Other Unix-like**: Fallback to standard library functions
//! - **Windows**: Not currently supported (PRs welcome!)
//!
//! ## Quick Start
//!
//! ```rust
//! use fdf::{Finder,DirEntry,SearchConfigError};
//! use std::sync::mpsc::Receiver;
//!
//! fn find_files() -> Result<Receiver<Vec<DirEntry>>, SearchConfigError> {
//!     let finder = Finder::init("/path/to/search")
//!         .pattern("*.rs")
//!         .keep_hidden(false)
//!         .max_depth(Some(3))
//!         .follow_symlinks(true)
//!         .canonicalise_root(true)  // Resolve the root to a full path
//!         .build()?;
//!
//!     finder.traverse()
//! }
//! ```
//!
//! ## Safety Considerations
//!
//! - **Symlink Following**: Enabled by `follow_symlinks(true)`, but use with caution
//!   to avoid infinite recursion (though we have guards against this!)
//! - **Depth Limits**: Always consider setting `max_depth` for large directory trees
//! - **Error Handling**: Use `show_errors(true)` to get diagnostic information about
//!   permission errors and other issues
//!
//! ## Examples
//!
//! ### Basic Usage
//! ```rust
//! # use fdf::{Finder};
//! let receiver = Finder::init(".")
//!     .pattern(".*txt")
//!     .build()
//!     .unwrap()
//!     .traverse()
//!     .unwrap();
//!
//! for batch in receiver {
//!     for entry in batch {
//!         println!("Found: {}", entry.to_string_lossy());
//!     }
//! }
//! ```
use core::num::NonZeroU16;
use rayon::prelude::*;
use std::{
    ffi::{OsStr, OsString},
    fs::metadata,
    os::unix::{ffi::OsStrExt as _, fs::MetadataExt as _},
    path::Path,
    sync::mpsc::{Receiver, Sender, channel as unbounded},
};

#[macro_use]
pub(crate) mod macros;

mod cli_helpers;

pub use cli_helpers::{FileTypeParser, SizeFilter, SizeFilterParser};

mod iter;
#[cfg(target_os = "linux")]
pub use iter::GetDents;
pub use iter::ReadDir;

#[cfg(target_os = "linux")]
mod syscalls;
#[cfg(target_os = "linux")]
pub use syscalls::{close_asm, getdents_asm, open_asm};

mod printer;
pub(crate) use printer::write_paths_coloured;

mod buffer;
mod test;
pub use buffer::{AlignedBuffer, ValueType};

mod memchr_derivations;
pub use memchr_derivations::{contains_zero_byte, find_char_in_word, memrchr};
mod direntry;
pub use direntry::DirEntry;

mod error;
pub use error::{DirEntryError, SearchConfigError};

mod types;

#[cfg(target_os = "linux")]
pub(crate) use types::SyscallBuffer;
pub use types::{BUFFER_SIZE, LOCAL_PATH_MAX, Result};
pub(crate) use types::{DirEntryFilter, FilterType, PathBuffer};

mod traits_and_conversions;
pub(crate) use traits_and_conversions::BytePath;
mod utils;
#[cfg(any(
    target_os = "linux",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd"
))]
pub use utils::dirent_const_time_strlen;

pub use utils::strlen;

mod glob;
pub use glob::glob_to_regex;
mod config;
pub use config::{FileTypeFilter, SearchConfig};
mod filetype;
use dashmap::DashSet;
pub use filetype::FileType;

//this allocator is more efficient than jemalloc through my testing(still better than system allocator)
#[cfg(all(any(target_os = "linux", target_os = "macos"), not(miri)))]
//miri doesnt support custom allocators
//not sure which platforms support this, BSD doesnt from testing, will test others as appropriate(GREAT DOCS!!!)
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

// The `Finder` struct is the main entry point for the file search.
// Its methods are exposed for building the search configuration

/// The main entry point for file system search operations.
///
/// `Finder` provides a high-performance, parallel file system traversal API
/// with configurable filtering and search criteria. It uses Rayon for parallel
/// execution and provides both synchronous and asynchronous result handling.
///
#[derive(Debug)]
pub struct Finder {
    /// Root directory path for the search operation
    pub(crate) root: OsString,
    /// Configuration for search criteria and filtering options
    pub(crate) search_config: SearchConfig,
    /// Optional custom filter function for advanced entry filtering
    pub(crate) filter: Option<DirEntryFilter>,
    /// Internal filter logic combining all filtering criteria
    pub(crate) custom_filter: FilterType,
    /// Filesystem device ID for same-filesystem constraint (optional)
    pub(crate) starting_filesystem: Option<u64>,
    /// Cache for (device, inode) pairs to prevent duplicate traversal with symlinks
    /// Uses `DashSet` for lock-free concurrent access
    pub(crate) inode_cache: Option<DashSet<(u64, u64)>>,
}
///The Finder struct is used to find files in your filesystem
impl Finder
//S is a generic type that implements BytesStorage trait aka  vec/arc/box/slimmerbox(alias to SlimmerBytes)
{
    #[must_use]
    #[inline]
    /// Create a new Finder instance.
    pub fn init<A: AsRef<OsStr>>(root: A) -> FinderBuilder {
        FinderBuilder::new(root)
    }

    #[inline]
    /// Traverse the directory tree starting from the root and return a receiver for the found entries.
    ///
    /// This method initiates a parallel directory traversal using Rayon. The traversal runs in a
    /// background thread and sends batches of directory entries through an unbounded channel.
    ///
    /// # Returns
    /// Returns a `Receiver<Vec<DirEntry>>` that will receive batches of directory entries
    /// as they are found during the traversal. The receiver can be used to iterate over the
    /// results as they become available.
    ///
    /// # Errors
    /// Returns `Err(SearchConfigError)` if:
    /// - The root path cannot be converted to a `DirEntry` (`TraversalError`)
    /// - The root directory is not traversible (`NotADirectory`)
    /// - The root directory is inaccessible due to permissions (`TraversalError`)
    ///
    ///
    /// # Performance Notes
    /// - Uses an unbounded channel to avoid blocking the producer thread
    /// - Entries are sent in batches to minimise channel contention
    /// - Traversal runs in parallel using Rayon's work-stealing scheduler
    pub fn traverse(self) -> core::result::Result<Receiver<Vec<DirEntry>>, SearchConfigError> {
        let (sender, receiver): (_, Receiver<Vec<DirEntry>>) = unbounded();

        // try to construct the starting directory entry
        let entry = DirEntry::new(&self.root).map_err(SearchConfigError::TraversalError)?;

        // only continue if it is traversible
        if entry.is_traversible() {
            // spawn the search in a new thread
            rayon::spawn(move || {
                self.process_directory(entry, &sender);
            });

            Ok(receiver)
        } else {
            Err(SearchConfigError::NotADirectory)
        }
    }

    #[inline]
    /// Prints search results to stdout with optional colouring and count limiting.
    ///
    /// This is a convenience method that handles the entire search, result collection,
    /// and formatted output in one call.
    ///
    /// # Arguments
    /// * `use_colours` - Enable ANSI colour output for better readability (it's always off if output does not support colours)
    /// * `result_count` - Optional limit on the number of results to display
    /// * `sort` Enable sorting of the end results (has a big computational cost)
    /// # Errors
    /// Either:
    /// Returns [`SearchConfigError::TraversalError`] if the search operation fails
    /// Returns [`SearchConfigError::IOError`] if the search operation fails
    pub fn print_results(
        self,
        use_colours: bool,
        result_count: Option<usize>,
        sort: bool,
    ) -> core::result::Result<(), SearchConfigError> {
        //TODO clean this up
        write_paths_coloured(self.traverse()?.iter(), result_count, use_colours, sort)
    }

    #[inline]
    /// Determines if a directory should be sent through the channel
    fn should_send_dir(&self, dir: &DirEntry) -> bool {
        self.search_config.keep_dirs && self.file_filter(dir) && dir.depth() != 0
    }

    #[inline]
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Exhaustive on traversible types"
    )]
    /// Determines if a directory should be traversed and caches the result
    fn should_traverse(&self, dir: &DirEntry) -> bool {
        match dir.file_type {
            // Regular directory - always traversible
            FileType::Directory => true,

            // Symlink - check if we should follow and if it points to a directory(the result is cached so the call isn't required each time.)
            FileType::Symlink if self.search_config.follow_symlinks => {
                dir.check_symlink_traversibility()
            }

            // All other file types or symlinks we don't follow
            _ => false,
        }
    }

    #[inline]
    /// Filters out hidden files if configured to do so
    const fn keep_hidden(&self, dir: &DirEntry) -> bool {
        !self.search_config.hide_hidden || !dir.is_hidden()
        // Some efficient boolean shortcircuits here to avoid checking
    }

    #[inline]
    /// Applies custom file filtering logic
    fn file_filter(&self, dir: &DirEntry) -> bool {
        (self.custom_filter)(&self.search_config, dir, self.filter)
    }

    #[inline]
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Exhaustive on traversible types"
    )]
    #[expect(
        clippy::ref_patterns,
        reason = "Borrowing doesn't work on this extreme lint"
    )]
    #[expect(clippy::option_if_let_else, reason = "Complicates it even more ")]
    /// Advanced filtering for directories and symlinks with filesystem constraints.
    ///
    /// Handles same-filesystem constraints, inode caching, and symlink resolution
    /// to prevent infinite loops and duplicate traversal.
    fn directory_or_symlink_filter(&self, dir: &DirEntry) -> bool {
        match dir.file_type {
            // Normal directories
            FileType::Directory => {
                match self.inode_cache {
                    None => {
                        // Fast path: only calls stat IFF self.starting_filesystem is Some
                        self.starting_filesystem.is_none_or(|start_dev| {
                            dir.get_stat()
                                .is_ok_and(|statted| start_dev == access_stat!(statted, st_dev))
                        })
                    }
                    Some(ref cache) => {
                        dir.get_stat().is_ok_and(|stat| {
                // Check same filesystem if enabled
                self.starting_filesystem.is_none_or(|start_dev| start_dev == access_stat!(stat, st_dev)) &&
                // Check if we've already traversed this inode
                cache.insert((access_stat!(stat, st_dev), access_stat!(stat, st_ino)))
            })
                    }
                }
            }

            // Symlinks that may point to directories
            // This could be optimised a bit, symlinks are a beast due to their complexity.
            FileType::Symlink if self.search_config.follow_symlinks => {
                dir.get_stat().is_ok_and(|stat| {
                FileType::from_stat(&stat) == FileType::Directory &&
                // Check filesystem boundary
                self.starting_filesystem.is_none_or(|start_dev| start_dev == access_stat!(stat, st_dev)) &&

                // Check if we've already traversed this inode
                self.inode_cache.as_ref().is_none_or(|cache| {
                    cache.insert((access_stat!(stat, st_dev), access_stat!(stat, st_ino))) &&
                // if we're traversing in the same root, then we'll find it anyway so skip it
                dir.get_realpath().is_ok_and(|path| !path.to_bytes().starts_with(self.root.as_bytes()))
                })
            })
            }

            // All other file types (files, non-followed symlinks, etc.)
            _ => false,
        }
    }

    #[expect(
        clippy::redundant_clone,
        reason = "we have to clone when sending dirs because it's being used to keep the iterator going.
         We don't want to collect an unnecessary vector, then into_iter and partition it,rather clone 1 directory than make an another vec!"
    )]
    #[inline]
    #[expect(clippy::print_stderr, reason = "only enabled if explicitly requested")]
    /// Recursively processes a directory, sending found files to a channel.
    ///
    /// This method uses a depth-first traversal with `rayon` to process directories
    /// in parallel.
    ///
    /// # Arguments
    /// * `dir` - The `DirEntry` representing the directory to process.
    /// * `sender` - A channel `Sender` to send batches of found `DirEntry`s.
    fn process_directory(&self, dir: DirEntry, sender: &Sender<Vec<DirEntry>>) {
        if !self.directory_or_symlink_filter(&dir) {
            return; //check for same filesystem/recursive symlinks etc, if so, return to avoid a loop/unnecessary info
        }

        let should_send_dir_or_symlink = self.should_send_dir(&dir); // If we've gotten here, the dir must be a directory!

        handle_depth_limit!(self, dir, should_send_dir_or_symlink, sender); // a convenience macro to clear up code here

        #[cfg(target_os = "linux")]
        // linux with getdents (only linux has stable ABI, so we can lower down to assembly/syscalls here, not for any other system tho)
        let direntries = dir.getdents(); // additionally, readdir internally calls stat on each file, which is expensive and unnecessary from testing!
        #[cfg(not(target_os = "linux"))]
        let direntries = dir.readdir(); // in theory I can use getattrlistbulk on macos(bsd potentially?), this has a LOT of complexity!
        // TODO! FIX THIS SEPARATE REPO https://github.com/alexcu2718/mac_os_getattrlistbulk_ls (I'll get around to this eventually)
        // I could get getdirentries alternatively for bsd

        match direntries {
            Ok(entries) => {
                let (dirs, mut files): (Vec<_>, Vec<_>) = entries
                    .filter(|entry| {
                        self.keep_hidden(entry)
                            && (self.should_traverse(entry) || self.file_filter(entry))
                    })
                    .partition(|ent| self.should_traverse(ent));

                // Process directories in parallel
                dirs.into_par_iter().for_each(|dirnt| {
                    self.process_directory(dirnt, sender);
                });

                // checking if we should send directories
                if should_send_dir_or_symlink {
                    files.push(dir.clone());
                    // luckily we're only cloning 1 directory/symlink, not anything more than that.
                }

                // We do batch sending to minimise contention of sending
                // as opposed to sending one at a time, which will cause tremendous locks
                send_files_if_not_empty!(self, files, sender); // a convenience macro to simplify the code 
            }
            Err(err) => {
                if self.search_config.show_errors {
                    eprintln!("Error accessing {}: {}", dir.to_string_lossy(), err);
                    //TODO! replace with logging eventually
                }
            }
        }
    }
}

/// A builder for creating a `Finder` instance with customisable options.
///
/// This builder allows you to set various options such as hiding hidden files, case sensitivity,
/// keeping directories in results, matching file extensions, setting maximum search depth,
/// following symlinks, and applying a custom filter function.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Naturally a builder will contain many bools"
)]
pub struct FinderBuilder {
    pub(crate) root: OsString,
    pub(crate) pattern: Option<String>,
    pub(crate) hide_hidden: bool,
    pub(crate) case_insensitive: bool,
    pub(crate) keep_dirs: bool,
    pub(crate) file_name_only: bool,
    pub(crate) extension_match: Option<Box<[u8]>>,
    pub(crate) max_depth: Option<NonZeroU16>,
    pub(crate) follow_symlinks: bool,
    pub(crate) filter: Option<DirEntryFilter>,
    pub(crate) size_filter: Option<SizeFilter>,
    pub(crate) file_type: Option<FileTypeFilter>,
    pub(crate) show_errors: bool,
    pub(crate) use_glob: bool,
    pub(crate) canonicalise: bool,
    pub(crate) same_filesystem: bool,
    pub(crate) thread_count: usize,
}

impl FinderBuilder {
    /// Creates a new `FinderBuilder` with required fields.
    ///
    /// # Arguments
    /// * `root` - The root directory to search
    pub fn new<A: AsRef<OsStr>>(root: A) -> Self {
        let thread_count = env!("CPU_COUNT").parse::<usize>().unwrap_or(1); //set default threadcount
        Self {
            root: root.as_ref().to_owned(),
            pattern: None,
            hide_hidden: true,
            case_insensitive: true,
            keep_dirs: false,
            file_name_only: true,
            extension_match: None,
            max_depth: None,
            follow_symlinks: false,
            filter: None,
            size_filter: None,
            file_type: None,
            show_errors: false,
            use_glob: false,
            canonicalise: false,
            same_filesystem: false,
            thread_count,
        }
    }
    #[must_use]
    /// Set the search pattern (regex or glob)
    pub fn pattern<P: AsRef<str>>(mut self, pattern: P) -> Self {
        self.pattern = Some(pattern.as_ref().into());
        self
    }

    #[must_use]
    /// Set whether to hide hidden files, defaults to true
    pub const fn keep_hidden(mut self, hide_hidden: bool) -> Self {
        self.hide_hidden = hide_hidden;
        self
    }
    #[must_use]
    /// Set case insensitive matching,defaults to true
    pub const fn case_insensitive(mut self, case_insensitive: bool) -> Self {
        self.case_insensitive = case_insensitive;
        self
    }
    #[must_use]
    /// Set whether to keep directories in results,defaults to false
    pub const fn keep_dirs(mut self, keep_dirs: bool) -> Self {
        self.keep_dirs = keep_dirs;
        self
    }
    #[must_use]
    /// Set whether to use short paths in regex matching, defaults to true
    pub const fn file_name_only(mut self, short_path: bool) -> Self {
        self.file_name_only = short_path;
        self
    }
    #[must_use]
    /// Set extension to match, defaults to no extension
    pub fn extension_match<C: AsRef<str>>(mut self, extension_match: C) -> Self {
        let ext = extension_match.as_ref().as_bytes();

        if ext.is_empty() {
            self.extension_match = None;
        } else {
            self.extension_match = Some(Box::from(ext));
        }

        self
    }
    #[must_use]
    /// Set maximum search depth
    pub const fn max_depth(mut self, max_depth: Option<u16>) -> Self {
        match max_depth {
            None => self,
            Some(num) => {
                if let Some(non_zero) = NonZeroU16::new(num) {
                    self.max_depth = Some(non_zero);
                } else {
                    // num == 0, remove depth limit by setting to None
                    self.max_depth = None;
                }
                self
            }
        }
    }
    #[must_use]
    /// Sets size-based filtering criteria.
    pub const fn filter_by_size(mut self, size_of: Option<SizeFilter>) -> Self {
        self.size_filter = size_of;
        self
    }

    /// Sets whether to follow symlinks (default: false).
    ///
    /// # Warning
    /// Enabling this may cause infinite recursion, although there are protections in place against it!
    #[must_use]
    pub const fn follow_symlinks(mut self, follow_symlinks: bool) -> Self {
        self.follow_symlinks = follow_symlinks;
        self
    }

    /// Set a custom filter
    #[must_use]
    pub const fn filter(mut self, filter: Option<DirEntryFilter>) -> Self {
        self.filter = filter;
        self
    }

    #[must_use]
    /// Sets file type filtering.
    pub const fn type_filter(mut self, filter: Option<FileTypeFilter>) -> Self {
        self.file_type = filter;
        self
    }

    #[must_use]
    /// Sets a glob pattern for regex matching, not a regex.
    pub const fn use_glob(mut self, use_glob: bool) -> Self {
        self.use_glob = use_glob;
        self
    }

    #[must_use]
    /// Set whether to show errors during traversal, defaults to false
    pub const fn show_errors(mut self, show_errors: bool) -> Self {
        self.show_errors = show_errors;
        self
    }

    #[must_use]
    /// Set whether to canonicalise (resolve absolute path) the root directory, defaults to false
    pub const fn canonicalise_root(mut self, canonicalise: bool) -> Self {
        self.canonicalise = canonicalise;
        self
    }

    #[must_use]
    #[allow(clippy::ref_patterns)]
    /// Set whether to escape any regexs in the string, defaults to false
    pub fn fixed_string(mut self, fixed_string: bool) -> Self {
        if let Some(ref patt) = self.pattern
            && fixed_string
        {
            self.pattern = Some(regex::escape(patt));
        }
        self
    }
    #[must_use]
    /// Set how many threads rayon is to use, defaults to max
    pub const fn thread_count(mut self, threads: usize) -> Self {
        self.thread_count = threads;

        self
    }

    #[must_use]
    /// Set whether to follow the same filesystem as root
    pub const fn same_filesystem(mut self, yesorno: bool) -> Self {
        self.same_filesystem = yesorno;
        self
    }

    /// Builds a [`Finder`] instance with the configured options.
    ///
    /// This method performs validation of all configuration parameters and
    /// initialises the necessary components for file system traversal.
    ///
    /// # Returns
    /// Returns `Ok(Finder<S>)` on successful configuration, or
    /// `Err(SearchConfigError)` if any validation fails.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The root path is not a directory or cannot be accessed
    /// - The root path cannot be canonicalised (when enabled)
    /// - The search pattern cannot be compiled to a valid regular expression
    /// - File system metadata cannot be retrieved (for same-filesystem tracking)
    pub fn build(self) -> core::result::Result<Finder, SearchConfigError> {
        // Resolve and validate the root directory
        let resolved_root = self.resolve_directory()?;
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(self.thread_count)
            .build_global(); //Skip the error, it only errors if it's already been initialised
        //we do this to avoid passing pools to every iterator (shared access locks etc.)

        let starting_filesystem = if self.same_filesystem {
            // Get the filesystem ID of the root directory directly
            let metadata = metadata(&resolved_root)?;
            Some(metadata.dev()) // dev() returns the filesystem ID on Unix
        } else {
            None
        };

        let search_config = SearchConfig::new(
            self.pattern.as_ref(),
            self.hide_hidden,
            self.case_insensitive,
            self.keep_dirs,
            self.file_name_only,
            self.extension_match,
            self.max_depth,
            self.follow_symlinks,
            self.size_filter,
            self.file_type,
            self.show_errors,
            self.use_glob,
        )?;

        let lambda: FilterType = |rconfig, rdir, rfilter| {
            {
                rfilter.is_none_or(|func| func(rdir))
                    && rconfig.matches_type(rdir)
                    && rconfig.matches_extension(&rdir.file_name())
                    && rconfig.matches_size(rdir)
                    && rconfig.matches_path(rdir, !rconfig.file_name_only)
            }
        };

        let inode_cache: Option<DashSet<(u64, u64)>> =
            (self.same_filesystem || self.follow_symlinks).then(DashSet::new);
        //Enable the cache if same file system too, this helps de-duplicate for free (since it's 1 stat call regardless)

        Ok(Finder {
            root: resolved_root,
            search_config,
            filter: self.filter,
            custom_filter: lambda,
            starting_filesystem,
            inode_cache,
        })
    }

    /// Resolves and validates the root directory path.
    ///
    /// This function handles:
    /// - Default to current directory (".") if root is empty
    /// - Validates that the path is a directory
    /// - Optionally canonicalises the path if canonicalise flag is set
    ///
    /// # Returns
    /// Returns the resolved directory path as an `OsString`
    ///
    /// # Errors
    /// Returns `SearchConfigError::NotADirectory` if the path is not a directory
    /// Returns `SearchConfigError::IoError` if canonicalisation fails
    fn resolve_directory(&self) -> core::result::Result<OsString, SearchConfigError> {
        let dir_to_use = if self.root.is_empty() {
            OsString::from(
                std::env::current_dir()
                    .and_then(|p| p.canonicalize())
                    .unwrap_or_else(|_| ".".into()),
            )
        } else {
            self.root.clone()
        };

        let path_check = Path::new(&dir_to_use);

        if !path_check.is_dir() {
            return Err(SearchConfigError::NotADirectory);
        }

        if self.canonicalise {
            path_check
                .canonicalize()
                .map(core::convert::Into::into)
                .map_err(SearchConfigError::IOError)
        } else {
            Ok(dir_to_use)
        }
    }
}
