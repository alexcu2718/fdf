/*!
 # fdf - A High-Performance Parallel File System Traversal Library

 `fdf` is a Rust library designed for efficient, parallel directory traversal
 with extensive filtering capabilities. It leverages Rayon for parallel processing
 and uses platform-specific optimisations for maximum performance.

 **This will be renamed before a 1.0!**

 ## Features

 - **Parallel Processing**: Utilises Rayon's work-stealing scheduler for concurrent
   directory traversal
 - **Platform Optimisations**: Linux/Android specific `getdents` system calls for optimal
   performance, with fallbacks for other platforms
 - **Flexible Filtering**: Support for multiple filtering criteria:
   - File name patterns (regex,glob)
   - File size ranges
   - File types (regular, directory, symlink, etc.)
   - File extensions
   - Hidden file handling
   - Custom filter functions
 - **Cycle Detection**: Automatic symlink cycle prevention using inode caching
 - **Depth Control**: Configurable maximum search depth
 - **Path Canonicalisation**: Optional path resolution to absolute paths
 - **Error Handling**: Configurable error reporting with detailed diagnostics

 ## Performance Characteristics

 - Uses mimalloc as global allocator on supported platforms for improved
   memory allocation performance
 - Batched result delivery to minimise channel contention
 - Zero-copy path handling where possible
 - Avoids unnecessary `stat` calls through careful API design
 - Makes up to 50% less `getdents` syscalls on linux/android, check the source code)

 ## Platform Support

 - **Linux/Android**: Optimised with direct `getdents` system calls
 - **BSD's/macOS**: Standard `readdir` with potential for future changes like `fts_open` (unlikely probably)
 - **Other Unix-like**: Fallback to standard library functions
 - **Windows**: Not currently supported (PRs welcome!)

 ## Quick Start

 ```rust
 use fdf::{Finder,DirEntry,SearchConfigError};
 use std::sync::mpsc::Receiver;

 fn find_files() -> Result<impl Iterator<Item = DirEntry>, SearchConfigError> {
    let finder = Finder::init("/path/to/search")
        .pattern("*.rs")
        .keep_hidden(false)
        .max_depth(Some(3))
        .follow_symlinks(true)
        .canonicalise_root(true) // Resolve the root to a full path
        .build()?;

    finder.traverse()
}
```

Setting custom filters example
```no_run

use fdf::{Finder, FileTypeFilter};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let finder = Finder::init("/var/log")
        .pattern("*.log")
        .keep_hidden(false)
        .type_filter(Some(FileTypeFilter::File))
        //set the custom filter
        .filter(Some(|entry| {
            entry.extension().is_some_and( |ext| ext.eq_ignore_ascii_case(b"log"))
        }))
        .build()?;

    let entries = finder.traverse()?;
    let mut count = 0;

    for entry in entries {
        count += 1;
        println!("Matched: {}", entry.as_path().display());
    }

    println!("Found {} log files", count);
    Ok(())
}
```
*/
#[cfg(any(target_os = "vita", target_os = "hurd", target_os = "nuttx"))]
compile_error!(
    "This application is not supported on PlayStation Vita/hurd/nuttx, It may be if I'm ever bothered!"
);

#[cfg(target_pointer_width = "32")]
compile_error!("Not supported on 32bit, I may do if a PR is sent!");

#[cfg(target_os = "windows")]
compile_error!("This application is not supported on Windows (yet)");

use rayon::prelude::*;
use std::{
    ffi::OsStr,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender, channel as unbounded},
    },
};

// Re-exports
pub use chrono;
pub use libc;

mod finderbuilder;
pub use finderbuilder::FinderBuilder;

#[macro_use]
pub(crate) mod macros;

mod cli_helpers;

pub use cli_helpers::{FileTypeParser, SizeFilter, SizeFilterParser, TimeFilter, TimeFilterParser};

mod iter;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub use iter::GetDents;
pub use iter::ReadDir;

#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub(crate) use libc::{dirent as dirent64, readdir as readdir64};

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) use libc::{dirent64, readdir64};

mod printer;
pub(crate) use printer::write_paths_coloured;

mod buffer;
mod test;
pub use buffer::{AlignedBuffer, ValueType};

mod memchr_derivations;
pub use memchr_derivations::{
    contains_zero_byte, find_char_in_word, find_last_char_in_word, find_zero_byte_u64, memrchr,
};
mod direntry;
pub use direntry::DirEntry;

mod error;
pub use error::{DirEntryError, FilesystemIOError, SearchConfigError, TraversalError};

mod types;

pub use types::FileDes;
pub use types::Result;

pub(crate) use types::{DirEntryFilter, FilterType};

mod utils;
pub(crate) use utils::BytePath;
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "emscripten",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "redox",
    target_os = "hermit",
    target_os = "fuchsia",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "aix",
    target_os = "hurd"
))]
pub use utils::dirent_const_time_strlen;

mod glob;
pub use glob::glob_to_regex;
mod config;
pub use config::{FileTypeFilter, SearchConfig};
mod filetype;
use dashmap::DashSet;
pub use filetype::FileType;

//this allocator is more efficient than jemalloc through my testing(still better than system allocator)
//miri doesnt support custom allocators
//not sure which platforms support this, BSD doesnt from testing
#[cfg(all(
    any(target_os = "linux", target_os = "android", target_os = "macos"),
    not(miri),
    not(debug_assertions)
))]
//miri doesnt support custom allocators
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc; //Please note, don't  use v3 it has weird bugs. I might try snmalloc in future.

/**
The `Finder` struct is the main entry point for the file search.
Its methods are exposed for building the search configuration

The main entry point for file system search operations.

`Finder` provides a high-performance, parallel file system traversal API
with configurable filtering and search criteria. It uses Rayon for parallel
execution and provides both synchronous and asynchronous result handling.

*/
#[derive(Debug)]
pub struct Finder {
    /// Root directory path for the search operation
    pub(crate) root: Box<OsStr>,
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
    /// Optionally Collected errors encountered during traversal
    pub(crate) errors: Option<Arc<Mutex<Vec<TraversalError>>>>,
}
///The Finder struct is used to find files in your filesystem
impl Finder {
    #[must_use]
    #[inline]
    /// Create a new Finder instance.
    pub fn init<A: AsRef<OsStr>>(root: A) -> FinderBuilder {
        FinderBuilder::new(root)
    }

    #[inline]
    #[must_use]
    /// Returns a reference to the underlying root
    pub const fn root_dir(&self) -> &OsStr {
        &self.root
    }

    #[inline]
    #[must_use]
    /**
     Returns the collected errors from the traversal

     Returns `Some(Vec<TraversalError>)` if error collection is enabled and errors occurred,
     or `None` if error collection is disabled or the lock failed
    */
    pub fn errors(&self) -> Option<Vec<TraversalError>> {
        self.errors
            .as_ref()
            .and_then(|arc| arc.lock().ok())
            .map(|guard| {
                guard
                    .iter()
                    .map(|te| TraversalError {
                        dir: te.dir.clone(),
                        error: DirEntryError::IOError(FilesystemIOError::from_io_error(
                            std::io::Error::other(te.error.to_string()),
                        )),
                    })
                    .collect()
            })
    }

    #[inline]
    /**
      Traverse the directory tree starting from the root and return a receiver for the found entries.

      This method initiates a parallel directory traversal using Rayon. The traversal runs in a
      background thread and sends batches of directory entries through an unbounded channel.

      # Returns
      Returns a `Receiver<Iterator<Item = DirEntry>>` that will receive batches of directory entries
      as they are found during the traversal. The receiver can be used to iterate over the
      results as they become available.

      # Errors
      Returns `Err(SearchConfigError)` if:
      - The root path cannot be converted to a `DirEntry` (`TraversalError`)
      - The root directory is not traversible (`NotADirectory`)
      - The root directory is inaccessible due to permissions (`TraversalError`)


      # Performance Notes
      - Uses an unbounded channel to avoid blocking the producer thread
      - Entries are sent in batches to minimise channel contention
      - Traversal runs in parallel using Rayon's work-stealing scheduler
    */
    pub fn traverse(
        self,
    ) -> core::result::Result<impl Iterator<Item = DirEntry>, SearchConfigError> {
        let (sender, receiver): (_, Receiver<Vec<DirEntry>>) = unbounded();

        // Construct starting entry
        let entry = DirEntry::new(self.root_dir()).map_err(SearchConfigError::TraversalError)?;

        if entry.is_traversible() {
            rayon::spawn(move || {
                self.process_directory(entry, &sender);
            });

            Ok(receiver.into_iter().flatten())
        } else {
            Err(SearchConfigError::NotADirectory)
        }
    }

    #[inline]
    /**
     Prints search results to stdout with optional colouring and count limiting.

     This is a convenience method that handles the entire search, result collection,
     and formatted output in a single call.

     # Arguments
     * `use_colours` - Enable ANSI colour output for better readability(if supported/going to a TTY)
     * `result_count` - Optional limit on the number of results to display
     * `sort` - Enable sorting of the final results (has significant computational cost)
     * `print_errors` - Print any errors collected (if errors were collected during traversal via the builder)

     # Errors
     Returns [`SearchConfigError::IOError`] if the search operation fails
    */
    pub fn print_results(
        self,
        use_colours: bool,
        result_count: Option<usize>,
        sort: bool,
        print_errors:bool
    ) -> core::result::Result<(), SearchConfigError> {
        let errors = self.errors.clone();
        let iter = self.traverse()?;
        write_paths_coloured(
            iter,
            result_count,
            use_colours,
            sort,
            print_errors,
            errors.as_ref(),
        )
    }

    #[inline]
    /// Determines if a directory should be sent through the channel
    fn should_send_dir(&self, dir: &DirEntry) -> bool {
        self.search_config.keep_dirs && dir.depth() != 0 && self.file_filter(dir)
        // Don't send root
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
        clippy::cast_sign_loss,
        reason = "Exhaustive on traversible types, Follows std treatment of dev devices"
    )]
    //https://doc.rust-lang.org/std/os/unix/fs/trait.MetadataExt.html#tymethod.dev
    #[allow(unfulfilled_lint_expectations)] //as above
    /**
     Advanced filtering for directories and symlinks with filesystem constraints.

     Handles same-filesystem constraints, inode caching, and symlink resolution
     to prevent infinite loops and duplicate traversal.
    */
    fn directory_or_symlink_filter(&self, dir: &DirEntry) -> bool {
        // This is a beast of a function to read, sorry!
        match dir.file_type {
        // Normal directories
        FileType::Directory => {
            self.inode_cache.as_ref().map_or_else(
                || {
                    // Fast path: only calls stat IFF self.starting_filesystem is Some
                    debug_assert!(!self.search_config.follow_symlinks,"we expect follow symlinks to be disabled when following this path");


                    self.starting_filesystem.is_none_or(|start_dev| {
                        dir.get_stat()
                            .is_ok_and(|statted| start_dev == access_stat!(statted, st_dev))
                    })
                },
                |cache| {
                    debug_assert!(self.search_config.follow_symlinks,"we expect follow symlinks to be enabled when following this path");


                    dir.get_stat().is_ok_and(|stat| {
                        // Check same filesystem if enabled
                        self.starting_filesystem.is_none_or(|start_dev| start_dev == access_stat!(stat, st_dev)) &&
                        // Check if we've already traversed this inode
                        cache.insert((access_stat!(stat, st_dev), access_stat!(stat, st_ino)))
                    })
                },
            )
        }

        // Symlinks that may point to directories
        // self.search_config.follow_symlinks <=> inode_cache is some
        FileType::Symlink
            if self.inode_cache.as_ref().is_some_and(|cache| {
                debug_assert!(self.search_config.follow_symlinks,"we expect follow symlinks to be enabled when following this path");


                dir.get_stat().is_ok_and(|stat| {
                    FileType::from_stat(&stat) == FileType::Directory &&
                    // Check filesystem boundary
                    self.starting_filesystem.is_none_or(|start_dev| start_dev == access_stat!(stat, st_dev)) &&
                    // Check if we've already traversed this inode
                    cache.insert((access_stat!(stat, st_dev), access_stat!(stat, st_ino)))
                })
            }) =>
        {
            true
        }

        // All other file types (files, non-followed symlinks, etc.)
        _ => false,
    }
    }
    #[inline]
    #[allow(
        clippy::let_underscore_must_use,
        reason = "errors only when channel is closed, not useful"
    )]
    fn handle_depth_limit(
        &self,
        dir: &DirEntry,
        should_send: bool,
        sender: &Sender<Vec<DirEntry>>,
    ) -> bool {
        if self
            .search_config
            .depth
            .is_some_and(|depth| dir.depth >= depth.get())
        {
            if should_send {
                let _ = sender.send(vec![dir.clone()]);
            } // cloning costs very little here.
            return false; // depth limit reached, stop processing
        }
        true // continue processing
    }

    #[inline]
    #[allow(
        clippy::let_underscore_must_use,
        reason = "errors only when channel is closed, not useful"
    )]
    /**
     Recursively processes a directory, sending found files to a channel.

     This method uses a depth-first traversal with `rayon` to process directories
     in parallel.

     # Arguments
     * `dir` - The `DirEntry` representing the directory to process.
     * `sender` - A channel `Sender` to send batches of found `DirEntry`s.
    */
    fn process_directory(&self, dir: DirEntry, sender: &Sender<Vec<DirEntry>>) {
        if !self.directory_or_symlink_filter(&dir) {
            return; //check for same filesystem/recursive symlinks etc, if so, return to avoid a loop/unnecessary info
        }

        let should_send_dir_or_symlink = self.should_send_dir(&dir); // If we've gotten here, the dir must be a directory!

        if !self.handle_depth_limit(&dir, should_send_dir_or_symlink, sender) {
            return;
        }

        #[cfg(any(target_os = "linux", target_os = "android"))]
        // linux with getdents (only linux/android allow direct syscalls)
        let direntries = dir.getdents(); // additionally, readdir internally calls stat on each file, which is expensive.
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        let direntries = dir.readdir();

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
                    files.push(dir);
                }

                // We do batch sending to minimise contention of sending
                // as opposed to sending one at a time, which will cause tremendous locks
                if !files.is_empty() {
                    let _ = sender.send(files); //Skip the error, the only errors happen when the channel is closed.
                }
            }
            Err(error) => {
                if let Some(errors_arc) = self.errors.as_ref() {
                    debug_assert!(
                        self.search_config.collect_errors,
                        "Sanity check, only collect errors when enabled"
                    );
                    // This will only show errors if collect errors is enabled
                    // Generally I don't like this approach due to the locking it can cause
                    // However, errors are VERY small typically hence this create negligible issues.
                    if let Ok(mut errors) = errors_arc.lock() {
                        errors.push(TraversalError { dir, error });
                    }
                }
            }
        }
    }
}
