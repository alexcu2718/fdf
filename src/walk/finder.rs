use crate::{
    DirEntryError, FilesystemIOError, SearchConfig, SearchConfigError, TraversalError,
    fs::{DirEntry, FileType},
    util::write_paths_coloured,
    walk::{DirEntryFilter, FilterType, finder_builder::FinderBuilder},
};
use dashmap::DashSet;
use rayon::iter::{IntoParallelIterator as _, ParallelIterator as _};
use std::{
    ffi::OsStr,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender, channel as unbounded},
    },
};

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

/// The Finder struct is used to find files in your filesystem
impl Finder {
    /// Create a new Finder instance.
    #[must_use]
    #[inline]
    pub fn init<A: AsRef<OsStr>>(root: A) -> FinderBuilder {
        FinderBuilder::new(root)
    }

    /// Returns a reference to the underlying root
    #[inline]
    #[must_use]
    pub const fn root_dir(&self) -> &OsStr {
        &self.root
    }

    /**
    Returns the collected errors from the traversal

    Returns `Some(Vec<TraversalError>)` if error collection is enabled and errors occurred,
    or `None` if error collection is disabled or the lock failed
    */
    #[inline]
    #[must_use]
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
    #[inline]
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

    /**
    Prints search results to stdout with optional colouring and count limiting.

    This is a convenience method that handles the entire search, result collection,
    and formatted output in a single call.

    # Arguments
    * `use_colours` - Enable ANSI colour output for better readability(if supported/going to a TTY)
    * `result_count` - Optional limit on the number of results to display
    * `sort` - Enable sorting of the final results (has significant computational cost)
    * `null_terminated` Enable a NUL as the terminator for non TTY output (for xargs compatibility with -0 flag)
    * `print_errors` - Print any errors collected (if errors were collected during traversal via the builder)

    # Errors
    Returns [`SearchConfigError::IOError`] if the search operation fails
    */
    #[inline]
    #[allow(clippy::fn_params_excessive_bools)] //convenience
    pub fn print_results(
        self,
        use_colours: bool,
        result_count: Option<usize>,
        sort: bool,
        null_terminated: bool,
        print_errors: bool,
    ) -> core::result::Result<(), SearchConfigError> {
        let errors = self.errors.clone();
        let iter = self.traverse()?;
        write_paths_coloured(
            iter,
            result_count,
            use_colours,
            sort,
            print_errors,
            null_terminated,
            errors.as_ref(),
        )
    }

    /// Determines if a directory should be sent through the channel
    #[inline]
    fn should_send_dir(&self, dir: &DirEntry) -> bool {
        self.search_config.keep_dirs && dir.depth() != 0 && self.file_filter(dir)
        // Don't send root
    }

    /// Determines if a directory should be traversed and caches the result
    #[inline]
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Exhaustive on traversible types"
    )]
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

    /// Filters out hidden files if configured to do so
    #[inline]
    const fn keep_hidden(&self, dir: &DirEntry) -> bool {
        !self.search_config.hide_hidden || !dir.is_hidden()
        // Some efficient boolean short circuits here to avoid checking
    }

    /// Applies custom file filtering logic
    #[inline]
    fn file_filter(&self, dir: &DirEntry) -> bool {
        (self.custom_filter)(&self.search_config, dir, self.filter)
    }

    /**
    Advanced filtering for directories and symlinks with filesystem constraints.

    Handles same-filesystem constraints, inode caching, and symlink resolution
    to prevent infinite loops and duplicate traversal.
    */
    #[inline]
    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::cast_sign_loss,
        reason = "Exhaustive on traversible types, Follows std treatment of dev devices"
    )]
    //https://doc.rust-lang.org/std/os/unix/fs/trait.MetadataExt.html#tymethod.dev
    #[allow(unfulfilled_lint_expectations)] // As above
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
            } // Cloning costs very little here.
            return false; // Depth limit reached, stop processing
        }
        true // Continue processing
    }

    /**
    Recursively processes a directory, sending found files to a channel.

    This method uses a depth-first traversal with `rayon` to process directories
    in parallel.

    # Arguments
    * `dir` - The `DirEntry` representing the directory to process.
    * `sender` - A channel `Sender` to send batches of found `DirEntry`s.
    */
    #[inline]
    #[allow(
        clippy::let_underscore_must_use,
        reason = "errors only when channel is closed, not useful"
    )]
    fn process_directory(&self, dir: DirEntry, sender: &Sender<Vec<DirEntry>>) {
        if !self.directory_or_symlink_filter(&dir) {
            return; // Check for same filesystem/recursive symlinks etc, if so, return to avoid a loop/unnecessary info
        }

        let should_send_dir_or_symlink = self.should_send_dir(&dir); // If we've gotten here, the dir must be a directory!

        if !self.handle_depth_limit(&dir, should_send_dir_or_symlink, sender) {
            return;
        }

        // linux with getdents (only linux/android allow direct syscalls)
        #[cfg(any(target_os = "linux", target_os = "android"))]
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

                // Checking if we should send directories
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
