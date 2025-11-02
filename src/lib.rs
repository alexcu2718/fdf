/*!
 # fdf - A High-Performance Parallel File System Traversal Library

 `fdf` is a Rust library designed for efficient, parallel directory traversal
 with extensive filtering capabilities. It leverages Rayon for parallel processing
 and uses platform-specific optimisations for maximum performance.

 **This will be renamed before a 1.0!**

 ## Features

 - **Parallel Processing**: Utilises Rayon's work-stealing scheduler for concurrent
   directory traversal
 - **Platform Optimisations**: Linux-specific `getdents` system calls for optimal
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
 - Makes up to 50% less `getdents` syscalls on linux (Not rigorously tested, check getdents `fill_buffer` docs)

 ## Platform Support

 - **Linux**: Optimised with direct `getdents` system calls
 - **Macos** Optimised with direct `getdirentries64` system calls
 - **BSD's**: Standard `readdir` with potential for future `getdirentries` optimisation.
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

 ## Safety Considerations

 - **Symlink Following**: Enabled by `follow_symlinks(true)`, but use with caution
   to avoid infinite recursion (though we have guards against this!)
 - **Depth Limits**: Always consider setting `max_depth` for large directory trees
 - **Error Handling**: Use `show_errors(true)` to get diagnostic information about
   permission errors and other issues

 ## Examples

 ### Basic Usage
 ```rust
 # use fdf::{Finder};
 let receiver = Finder::init(".")
     .pattern(".*txt")
     .build()
     .unwrap()
     .traverse()
     .unwrap();

 for entry in receiver {
       println!("Found: {}", entry.to_string_lossy());
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
            let path = entry.as_path();
            path.extension()
                .and_then(|ext| ext.to_str()) //I don't recommend doing this, since it can be converted to bytes!
                .map_or(false, |ext| ext.eq_ignore_ascii_case("log"))
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

#[cfg(any(target_os = "vita", target_os = "hurd"))]
compile_error!(
    "This application is not supported on PlayStation Vita/hurd, It may be if I'm ever bothered!"
);

#[cfg(target_os = "windows")]
compile_error!("This application is not supported on Windows (yet)");

use rayon::prelude::*;

use std::{
    ffi::OsStr,
    os::unix::ffi::OsStrExt as _,
    sync::mpsc::{Receiver, Sender, channel as unbounded},
};

// Re-exports
pub use chrono;
pub use libc;

mod finderbuilder;
pub use finderbuilder::FinderBuilder;

#[macro_use]
pub(crate) mod macros;

mod cli_helpers;

pub use cli_helpers::{FileTypeParser, SizeFilter, SizeFilterParser};

mod iter;
#[cfg(target_os = "linux")]
pub use iter::GetDents;
pub use iter::ReadDir;

mod printer;
pub(crate) use printer::write_paths_coloured;

mod buffer;
mod test;
pub use buffer::{AlignedBuffer, ValueType};

mod memchr_derivations;
pub use memchr_derivations::{contains_zero_byte, find_char_in_word, find_zero_byte_u64, memrchr};
mod direntry;
pub use direntry::DirEntry;

mod error;
pub use error::{DirEntryError, SearchConfigError};

mod types;

#[cfg(target_os = "linux")]
pub use types::BUFFER_SIZE;
pub use types::FileDes;
pub use types::Result;

pub(crate) use types::{DirEntryFilter, FilterType};

mod utils;
pub(crate) use utils::BytePath;
#[cfg(any(
    target_os = "linux",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "android"
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
#[cfg(all(
    any(target_os = "linux", target_os = "macos", target_os = "android"),
    not(miri),
    not(debug_assertions)
))]
//miri doesnt support custom allocators
//not sure which platforms support this, BSD doesnt from testing, will test others as appropriate(GREAT DOCS!!!)
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

//use std::num::
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
      -`use_colours` - Enable ANSI colour output for better readability
                       (automatically disabled if output does not support colours)
      -`result_count` - Optional limit on the number of results to display
      =`sort` - Enable sorting of the final results (has significant computational cost)

     # Errors
     -Returns [`SearchConfigError::IOError`] if the search operation fails
    */
    pub fn print_results(
        self,
        use_colours: bool,
        result_count: Option<usize>,
        sort: bool,
    ) -> core::result::Result<(), SearchConfigError> {
        //TODO clean this up
        write_paths_coloured(self.traverse()?, result_count, use_colours, sort)
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
    /**
     Advanced filtering for directories and symlinks with filesystem constraints.

     Handles same-filesystem constraints, inode caching, and symlink resolution
     to prevent infinite loops and duplicate traversal.
    */
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
                // if the path is also in the root of the search directory skip it.
                 dir.get_realpath(|path| {
                    Ok(!path.to_bytes().starts_with(self.root_dir().as_bytes()))
                }).unwrap_or(false) &&
                // Check filesystem boundary
                self.starting_filesystem.is_none_or(|start_dev| start_dev == access_stat!(stat, st_dev)) &&
                // Check if we've already traversed this inode
                self.inode_cache.as_ref().is_none_or(|cache| {
                cache.insert((access_stat!(stat, st_dev), access_stat!(stat, st_ino)))
                //TODO? investigate the semantics of hidden file and relative directories
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

        handle_depth_limit!(self, dir, should_send_dir_or_symlink, sender); // a convenience macro to clear up code here

        #[cfg(target_os = "linux")]
        // linux with getdents (only linux/android allow direct syscalls, add this for android too when I can be bothered!) TODO!!
        let direntries = dir.getdents(); // additionally, readdir internally calls stat on each file, which is expensive and unnecessary from testing!
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let direntries = dir.readdir();
        #[cfg(target_os = "macos")]
        let direntries = dir.getdirentries();

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
