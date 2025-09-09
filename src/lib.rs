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
//! - **Memory Efficiency**: Multiple storage backends (Vec, Arc, Box, `SlimmerBytes` )
//!   for different memory/performance trade-offs
//! - **Cycle Detection**: Automatic symlink cycle prevention using inode caching
//! - **Depth Control**: Configurable maximum search depth
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
//! use fdf::{Finder, SlimmerBytes,DirEntryError,DirEntry};
//! use std::sync::mpsc::Receiver;
//!
//! fn find_files() -> Result<Receiver<Vec<DirEntry<SlimmerBytes>>>, DirEntryError> {
//!     let finder = Finder::<SlimmerBytes>::init("/path/to/search", "*.rs")
//!         .keep_hidden(false)
//!         .max_depth(Some(3))
//!         .follow_symlinks(true)
//!         .build()?;
//!
//!     finder.traverse()
//! }
//! ```
//!
//! ## Storage Backends
//!
//! The library supports multiple storage types through the `BytesStorage` trait:
//!
//! - `Vec<u8>`: Standard vector storage
//! - `Arc<[u8]>`: Shared ownership for reduced copying
//! - `Box<[u8]>`: Owned boxed slice
//! - `SlimmerBytes`: Custom optimised storage type
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
//! # use fdf::{Finder, SlimmerBytes};
//! let receiver = Finder::<SlimmerBytes>::init(".", ".*txt")
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

use rayon::prelude::*;
use std::{
    ffi::{OsStr, OsString},
    sync::mpsc::{Receiver, Sender, channel as unbounded},
};
#[macro_use]
pub(crate) mod cursed_macros;

mod size_filter;

pub use size_filter::SizeFilter;

mod iter;
pub(crate) use iter::DirIter;

#[cfg(target_os = "linux")]
mod syscalls;
#[cfg(target_os = "linux")]
pub use syscalls::{close_asm, getdents_asm, open_asm};

mod buffer;
mod test;
pub use buffer::{AlignedBuffer, ValueType};

mod memchr_derivations;
pub use memchr_derivations::{contains_zero_byte, find_char_in_word, memrchr};
mod direntry;
pub use direntry::DirEntry;

mod error;
pub use error::DirEntryError;

mod custom_types_result;

#[cfg(target_os = "linux")]
pub(crate) use custom_types_result::SyscallBuffer;
pub use custom_types_result::{
    BUFFER_SIZE, BytesStorage, LOCAL_PATH_MAX, OsBytes, Result, SlimmerBytes,
};
pub(crate) use custom_types_result::{DirEntryFilter, FilterType, PathBuffer};

mod traits_and_conversions;
pub(crate) use traits_and_conversions::BytePath;
mod utils;
#[cfg(any(target_os = "linux", target_os = "illumos", target_os = "solaris"))]
pub use utils::dirent_const_time_strlen;
pub use utils::{modified_unix_time_to_datetime, strlen};

mod glob;
pub use glob::glob_to_regex;
mod config;
pub use config::{FileTypeFilter, SearchConfig};
mod filetype;
pub use filetype::FileType;

use dashmap::DashSet;
use std::sync::LazyLock;

//this allocator is more efficient than jemalloc through my testing(still better than system allocator)
#[cfg(all(any(target_os = "linux", target_os = "macos"), not(miri)))]
//miri doesnt support custom allocators
//not sure which platforms support this, BSD doesnt from testing, will test others as appropriate(GREAT DOCS!!!)
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

// The `Finder` struct is the main entry point for the file search.
// Its methods are exposed for building the search configuration
#[derive(Debug)]
/// Creates a new `FinderBuilder` with required fields.
///
/// # Arguments
/// * `root` - The root directory to search
/// * `pattern` - The glob pattern to match files against
pub struct Finder<S>
where
    S: BytesStorage,
{
    pub(crate) root: OsString,
    pub(crate) search_config: SearchConfig,
    pub(crate) filter: Option<DirEntryFilter<S>>,
    pub(crate) custom_filter: FilterType<S>,
}
///The Finder struct is used to find files in a directory.
impl<S> Finder<S>
//S is a generic type that implements BytesStorage trait aka  vec/arc/box/slimmerbox(alias to SlimmerBytes)
where
    S: BytesStorage + 'static + Clone + Send,
{
    #[must_use]
    #[inline]
    /// Create a new Finder instance.
    pub fn init<A: AsRef<OsStr>, B: AsRef<str>>(root: A, pattern: B) -> FinderBuilder<S> {
        FinderBuilder::new(root, pattern)
    }

    #[must_use]
    #[inline]
    /// Set a filter function to filter out entries.
    pub fn with_type_filter(mut self, filter: DirEntryFilter<S>) -> Self {
        self.filter = Some(filter);
        self
    }

    #[inline]
    /// Traverse the directory tree starting from the root and return a receiver for the found entries.
    ///
    /// This method initiates a parallel directory traversal using Rayon. The traversal runs in a
    /// background thread and sends batches of directory entries through an unbounded channel.
    ///
    /// # Returns
    /// Returns a `Receiver<Vec<DirEntry<S>>>` that will receive batches of directory entries
    /// as they are found during the traversal. The receiver can be used to iterate over the
    /// results as they become available.
    ///
    /// # Errors
    /// Returns `Err(DirEntryError::InvalidPath)` if:
    /// - The root path cannot be converted to a `DirEntry`
    /// - The root directory is not traversible (e.g., not a directory or inaccessible(usually permissions based))
    ///
    /// # Performance Notes
    /// - Uses an unbounded channel to avoid blocking the producer thread
    /// - Entries are sent in batches to minimize channel contention
    /// - Traversal runs in parallel using Rayon's work-stealing scheduler
    pub fn traverse(self) -> Result<Receiver<Vec<DirEntry<S>>>> {
        let (sender, receiver): (_, Receiver<Vec<DirEntry<S>>>) = unbounded();

        // try to construct the starting directory entry
        let entry = DirEntry::new(&self.root)?;

        // only continue if it is traversible
        if entry.is_traversible() {
            // spawn the search in a new thread
            rayon::spawn(move || {
                self.process_directory(entry, &sender);
            });

            Ok(receiver)
        } else {
            Err(DirEntryError::NotADirectory)
        }
    }

    #[inline]
    fn directory_or_symlink_filter(&self, dir: &DirEntry<S>) -> bool {
        /// A cache to hold inodes for directories and symlinks
        /// Use a lock free Hashset to accomplish this
        static INODE_CACHE: LazyLock<DashSet<u64>> = LazyLock::new(DashSet::new);

        // Handle normal directories first
        if dir.is_dir() {
            if !self.search_config.follow_symlinks {
                return true; // Do not cause any cache operations if not traversing symlinks
            }
            INODE_CACHE.insert(dir.ino());
            return true;
        }

        // Handle symlinks pointing to directories
        //Check if it's a directory and not already in the cache, then apply the file filter
        // This is quite cheap because there are so few in a system anyway
        // TODO! this could be optimised, it's just very tricky!
        if self.search_config.follow_symlinks && dir.is_symlink() {
            return dir.get_stat().is_ok_and(|stat| {
                FileType::from_stat(&stat) == FileType::Directory
                    && INODE_CACHE.insert(access_stat!(stat, st_ino))
                    && self.file_filter(dir)
            });
        }

        false
    }

    #[inline]
    fn keep_hidden(&self, dir: &DirEntry<S>) -> bool {
        !self.search_config.hide_hidden || !dir.is_hidden()
    }

    #[inline]
    fn file_filter(&self, dir: &DirEntry<S>) -> bool {
        (self.custom_filter)(&self.search_config, dir, self.filter)
    }
    #[inline]
    #[expect(
        clippy::print_stderr,
        reason = "only enabled if explicitly requested or a very unusual error!"
    )]
    /// Recursively processes a directory, sending found files to a channel.
    ///
    /// This method uses a depth-first traversal with `rayon` to process directories
    /// in parallel.
    ///
    /// # Arguments
    /// * `dir` - The `DirEntry` representing the directory to process.
    /// * `sender` - A channel `Sender` to send batches of found `DirEntry`s.
    fn process_directory(&self, dir: DirEntry<S>, sender: &Sender<Vec<DirEntry<S>>>) {
        let should_send_dir_or_symlink = // If we've gotten here, the dir must be a directory!
        self.search_config.keep_dirs && self.file_filter(&dir) && dir.depth() != 0; // don't send root directory

        if self
            .search_config
            .depth
            .is_some_and(|depth| dir.depth >= depth)
        {
            if should_send_dir_or_symlink {
                let _ = sender.send(vec![dir]);
            } // have to put into a vec, this doesn't matter because this only happens when we depth limit
            // I purposely ignore the result here because of pipe errors/other errors (like -n 10) that I should probably log.
            // TODO-MAYBE

            return; // stop processing this directory if depth limit is reached
        }

        #[cfg(target_os = "linux")]
        // linux with getdents (only linux has stable ABI, so we can lower down to assembly here, not for any other system tho)
        let direntries = dir.getdents(); // additionally, readdir internally calls stat on each file, which is expensive and unnecessary from testing!

        #[cfg(not(target_os = "linux"))]
        let direntries = dir.readdir(); // in theory I can use getattrlistbulk on macos, this has a LOT of complexity!
        // https://man.freebsd.org/cgi/man.cgi?query=getattrlistbulk&sektion=2&manpath=macOS+13.6.5 TODO!
        // TODO! FIX THIS SEPARATE REPO https://github.com/alexcu2718/mac_os_getattrlistbulk_ls
        // THIS REQUIRES A LOT MORE WORK TO VALIDATE SAFETY BEFORE I CAN USE IT, IT'S ALSO VERY ROUGH SINCE MACOS API IS TERRIBLE

        // These lambdas make the code a bit easier to follow, I might define them as functions later, TODO-MAYBE!
        // directory or symlink lambda
        // Keep all directories (and symlinks if following them)

        match direntries {
            Ok(entries) => {
                // This boolean logic is designed for efficiency through short-circuiting.
                // 1. We first check `keep_hidden`. If a file is hidden and `hide_hidden` is true,
                //    the entire expression immediately evaluates to `false`, and we move to the next entry.
                // 2. If the entry is not hidden (or `hide_hidden` is false), we then check
                //    `(self.directory_or_symlink_filter(entry) || self.file_filter(entry))`.
                // 3. This part uses another short-circuit. checks if the entry is
                //    a directory or a symlink we should follow. If it is, the right side (`file_filter`)
                //    is not evaluated, which avoids an expensive call to `file_filter` on directories.
                // 4. If the entry is not a directory/symlink, we then run the `file_filter`, which
                //    contains the main logic for filtering files (e.g., by name, size, filetype etc).
                let (dirs, mut files): (Vec<_>, Vec<_>) = entries
                    .filter(|entry| {
                        self.keep_hidden(entry)
                            && (self.directory_or_symlink_filter(entry) || self.file_filter(entry))
                    })
                    .partition(|ent| self.directory_or_symlink_filter(ent));

                // Process directories in parallel
                dirs.into_par_iter().for_each(|dirnt| {
                    self.process_directory(dirnt, sender);
                });

                // checking if we should send directories
                if should_send_dir_or_symlink {
                    #[expect(
                        clippy::redundant_clone,
                        reason = "we have to clone here unfortunately because it's being used to keep the iterator going. We don't want to collect a whole allocation!"
                    )]
                    files.push(dir.clone());
                    // luckily we're only cloning 1 directory/symlink, not anything more than that.
                }

                // We do batch sending to minimise contention of sending
                // as opposed to sending one at a time, which will cause tremendous locks
                if !files.is_empty() {
                    let _ = sender.send(files);
                }
            }
            Err(
                err @ (DirEntryError::TemporarilyUnavailable
                | DirEntryError::AccessDenied(_)
                | DirEntryError::InvalidPath
                | DirEntryError::TooManySymbolicLinks),
            ) => {
                if self.search_config.show_errors {
                    eprintln!("Error accessing {}: {}", dir.to_string_lossy(), err);
                }
            }
            Err(err) => {
                eprintln!("Unspecified directory entry error at {dir}: {err}. Please report this.",);
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
pub struct FinderBuilder<S>
where
    S: BytesStorage,
{
    pub(crate) root: OsString,
    pub(crate) pattern: String,
    pub(crate) hide_hidden: bool,
    pub(crate) case_insensitive: bool,
    pub(crate) keep_dirs: bool,
    pub(crate) file_name_only: bool,
    pub(crate) extension_match: Option<Box<[u8]>>,
    pub(crate) max_depth: Option<u16>,
    pub(crate) follow_symlinks: bool,
    pub(crate) filter: Option<DirEntryFilter<S>>,
    pub(crate) size_filter: Option<SizeFilter>,
    pub(crate) file_type: Option<FileTypeFilter>,
    pub(crate) show_errors: bool,
    pub(crate) use_glob: bool,
}

impl<S> FinderBuilder<S>
where
    S: BytesStorage + 'static + Clone + Send,
{
    /// Creates a new `FinderBuilder` with required fields.
    ///
    /// # Arguments
    /// * `root` - The root directory to search
    /// * `pattern` - The glob pattern to match files against
    pub fn new<A: AsRef<OsStr>, B: AsRef<str>>(root: A, pattern: B) -> Self {
        Self {
            root: root.as_ref().to_owned(),
            pattern: pattern.as_ref().to_owned(),
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
        }
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
    /// Set extension to match
    pub fn extension_match<C: AsRef<str>>(mut self, extension_match: Option<C>) -> Self {
        self.extension_match = extension_match.map(|x| x.as_ref().as_bytes().into());
        self
    }
    #[must_use]
    /// Set maximum search depth
    pub const fn max_depth(mut self, max_depth: Option<u16>) -> Self {
        self.max_depth = max_depth;
        self
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
    pub const fn filter(mut self, filter: Option<DirEntryFilter<S>>) -> Self {
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

    /// Builds the Finder instance with the configured options.
    ///
    /// # Returns
    /// A `Result` containing the configured `Finder` instance
    ///
    /// # Errors
    /// Returns an error if the search pattern cannot be compiled to a valid regex
    pub fn build(self) -> Result<Finder<S>> {
        let search_config = SearchConfig::new(
            self.pattern,
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

        let lambda: FilterType<S> = |rconfig, rdir, rfilter| {
            {
                rfilter.is_none_or(|func| func(rdir))
                    && rconfig.matches_type(rdir)
                    && rconfig.matches_extension(&rdir.file_name())
                    && rconfig.matches_size(rdir)
                    && rconfig.matches_path(rdir, !rconfig.file_name_only)
            }
        };

        Ok(Finder {
            root: self.root,
            search_config,
            filter: self.filter,
            custom_filter: lambda,
        })
    }
}
