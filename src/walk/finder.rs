use crate::{
    DirEntryError, FilesystemIOError, SearchConfig, SearchConfigError, TraversalError,
    fs::{DirEntry, FileType},
    util::PrinterBuilder,
    walk::{DirEntryFilter, FilterType, finder_builder::FinderBuilder},
};
use core::{
    mem,
    num::NonZeroUsize,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use crossbeam_channel::{Receiver, SendError, Sender, bounded};
use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use dashmap::DashSet;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::{
    ffi::OsStr,
    path::Path,
    sync::{Arc, Mutex},
    thread,
};

/**
The `Finder` struct is the main entry point for the file search.
Its methods are exposed for building the search configuration

The main entry point for file system search operations.

`Finder` provides a high-performance, parallel file system traversal API
with configurable filtering and search criteria. It uses a worker pool for
parallel execution and provides both synchronous and asynchronous result handling.
*/
#[derive(Debug)]
pub struct Finder {
    /// Root directory path for the search operation
    pub(crate) root: Box<OsStr>,
    /// Configuration for search criteria and filtering options
    pub(crate) search_config: SearchConfig,
    /// Optional custom filter function for advanced entry filtering
    pub(crate) custom_filter: Option<DirEntryFilter>,
    /// Internal filter logic combining all filtering criteria
    pub(crate) file_filter: FilterType,
    /// Filesystem device ID for same-filesystem constraint (optional)
    pub(crate) starting_filesystem: Option<u64>,
    /// Cache for (device, inode) pairs to prevent duplicate traversal with symlinks
    /// Uses `DashSet` for lock-free concurrent access
    pub(crate) inode_cache: Option<DashSet<(u64, u64)>>,
    /// Optionally Collected errors encountered during traversal
    pub(crate) errors: Option<Arc<Mutex<Vec<TraversalError>>>>,
    /// Maximum worker threads used for traversal
    pub(crate) thread_count: NonZeroUsize,
}

/// Maximum size of a result batch before flushing to the receiver.
const RESULT_BATCH_LIMIT: usize = 256; //TODO TEST DIFFERENT VALUES FOR THIS (256 seems to perform best?)
/// Channel capacity multiplier for result buffering.
const RESULT_CHANNEL_FACTOR: usize = 2;

/// Wrapper that sends batches of items at once over a channel.
struct BatchSender {
    items: Vec<DirEntry>,
    tx: Sender<Vec<DirEntry>>,
    limit: usize,
}

impl BatchSender {
    fn new(tx: Sender<Vec<DirEntry>>, limit: usize) -> Self {
        Self {
            items: Vec::with_capacity(limit),
            tx,
            limit,
        }
    }

    fn send(&mut self, item: DirEntry) -> Result<(), SendError<Vec<DirEntry>>> {
        self.items.push(item);
        if self.items.len() >= self.limit {
            let batch = mem::take(&mut self.items);
            self.tx.send(batch)?;
            self.items = Vec::with_capacity(self.limit);
        }

        Ok(())
    }

    fn flush(&mut self) -> Result<(), SendError<Vec<DirEntry>>> {
        if self.items.is_empty() {
            return Ok(());
        }

        let batch = mem::take(&mut self.items);
        self.tx.send(batch)?;
        self.items = Vec::with_capacity(self.limit);
        Ok(())
    }
}
// on drop, we need to flush the buffers.
impl Drop for BatchSender {
    fn drop(&mut self) {
        if self.flush().is_err() {}
    }
}

struct PendingGuard<'guard> {
    pending: &'guard AtomicUsize,
    shutdown_flag: &'guard AtomicBool,
}

impl<'guard> PendingGuard<'guard> {
    const fn new(pending: &'guard AtomicUsize, shutdown_flag: &'guard AtomicBool) -> Self {
        Self {
            pending,
            shutdown_flag,
        }
    }
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        //the thread that performs the final decrement will see the up‑to‑date value
        // (thanks to the acquire half) and can then safely signal shutdown.
        let remaining = self.pending.fetch_sub(1, Ordering::AcqRel) - 1;
        if remaining == 0 {
            self.shutdown_flag.store(true, Ordering::Relaxed)
        }
    }
}

#[derive(Clone)]
struct WorkItem {
    dir: DirEntry,
    ignore_ctx: Arc<IgnoreContext>,
}

struct IgnoreContext {
    parent: Option<Arc<IgnoreContext>>,
    matcher: Option<Arc<Gitignore>>,
    repo_active: bool,
}

impl IgnoreContext {
    fn empty() -> Arc<Self> {
        Arc::new(Self {
            parent: None,
            matcher: None,
            repo_active: false,
        })
    }

    fn child(parent: Arc<Self>, matcher: Option<Arc<Gitignore>>, repo_active: bool) -> Arc<Self> {
        Arc::new(Self {
            parent: Some(parent),
            matcher,
            repo_active,
        })
    }
}

fn find_task(
    local: &Worker<WorkItem>,
    injector: &Injector<WorkItem>,
    stealers: &[Stealer<WorkItem>],
) -> Option<WorkItem> {
    if let Some(task) = local.pop() {
        return Some(task);
    }

    loop {
        match injector.steal_batch_and_pop(local) {
            Steal::Success(task) => return Some(task),
            Steal::Retry => continue,
            Steal::Empty => {}
        }

        let mut retry = false;
        for stealer in stealers {
            match stealer.steal() {
                Steal::Success(task) => return Some(task),
                Steal::Retry => retry = true,
                Steal::Empty => {}
            }
        }

        if !retry {
            return None;
        }
    }
}
struct WorkerContext<'ctx> {
    local: &'ctx Worker<WorkItem>,
    pending: &'ctx AtomicUsize,
    shutdown_flag: &'ctx AtomicBool,
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
    #[must_use]
    #[allow(clippy::missing_inline_in_public_items)]
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
    Traverse the directory tree starting from the root and return an iterator for the found entries.

    This method initiates a parallel directory traversal using a worker pool. The traversal runs
    in background threads and sends batches of directory entries through a bounded channel.

    # Returns
    Returns an iterator that yields directory entries as they are discovered by the background
    worker threads.

    # Errors
    Returns `Err(SearchConfigError)` if:
    - The root path cannot be converted to a `DirEntry` (`TraversalError`)
    - The root directory is not traversible (`NotADirectory`)
    - The root directory is inaccessible due to permissions (`TraversalError`)


    # Performance Notes
    - Uses a bounded channel to provide backpressure when the consumer slows down
    - Entries are sent in batches to minimise channel contention
    - Traversal runs in parallel using fixed worker threads
    */
    #[inline]
    pub fn traverse(
        self,
    ) -> core::result::Result<impl Iterator<Item = DirEntry>, SearchConfigError> {
        let thread_count = self.thread_count.get();
        let result_buffer = thread_count.saturating_mul(RESULT_CHANNEL_FACTOR).max(1);
        let (sender, receiver): (_, Receiver<Vec<DirEntry>>) = bounded(result_buffer);
        let injector = Arc::new(Injector::new());
        let pending = Arc::new(AtomicUsize::new(1));
        let shutdown_flag = Arc::new(AtomicBool::new(false));

        let mut workers = Vec::with_capacity(thread_count);
        let mut stealers = Vec::with_capacity(thread_count);
        for _ in 0..thread_count {
            let worker = Worker::new_lifo();
            stealers.push(worker.stealer());
            workers.push(worker);
        }
        let stealers_shared = Arc::new(stealers);

        // Construct starting entry
        let entry = DirEntry::new(self.root_dir()).map_err(SearchConfigError::TraversalError)?;

        if entry.is_traversible() {
            let finder = Arc::new(self);
            injector.push(WorkItem {
                dir: entry,
                ignore_ctx: IgnoreContext::empty(),
            });

            for (index, worker) in workers.into_iter().enumerate() {
                let finder_shared = Arc::clone(&finder);
                let sender_shared = sender.clone();
                let pending_shared = Arc::clone(&pending);
                let shutdown_flag_shared = Arc::clone(&shutdown_flag);
                let injector_shared = Arc::clone(&injector);
                let stealers_pool = Arc::clone(&stealers_shared);

                thread::spawn(move || {
                    let mut batch_sender = BatchSender::new(sender_shared, RESULT_BATCH_LIMIT);
                    let mut local_stealers =
                        Vec::with_capacity(stealers_pool.len().saturating_sub(1));
                    for (idx, stealer) in stealers_pool.iter().enumerate() {
                        if idx != index {
                            local_stealers.push(stealer.clone());
                        }
                    }

                    loop {
                        if shutdown_flag_shared.load(Ordering::Relaxed)
                            && worker.is_empty()
                            && injector_shared.is_empty()
                        {
                            break;
                        }

                        let Some(work_item) = find_task(&worker, &injector_shared, &local_stealers)
                        else {
                            if shutdown_flag_shared.load(Ordering::Relaxed) {
                                break;
                            }
                            thread::yield_now();
                            continue;
                        };

                        let _pending_guard =
                            PendingGuard::new(&pending_shared, &shutdown_flag_shared);

                        let ctx = WorkerContext {
                            local: &worker,
                            pending: &pending_shared,
                            shutdown_flag: &shutdown_flag_shared,
                        };

                        finder_shared.process_directory(work_item, &mut batch_sender, &ctx);
                    }
                });
            }

            Ok(receiver.into_iter().flatten())
        } else {
            Err(SearchConfigError::NotADirectory)
        }
    }

    /**
    Build a [`PrinterBuilder`] from this finder.

    This is a convenience method that starts traversal and returns a configured printer
    builder containing:
    - the traversal result iterator
    - collected error storage (when enabled in the finder configuration)

    Use the returned builder to configure output behaviour (limit, sorting, colour,
    null-terminated output, and error printing) and then call `.print()`.

    # Errors
    Returns a [`SearchConfigError`] if traversal setup fails.
    */
    #[inline]
    pub fn build_printer(
        self,
    ) -> core::result::Result<PrinterBuilder<impl Iterator<Item = DirEntry>>, SearchConfigError>
    {
        let errors = self.errors.clone();
        Ok(PrinterBuilder::new(self.traverse()?).errors(errors))
    }

    /// Determines if a directory should be sent through the channel
    #[inline]
    fn should_send_dir(&self, dir: &DirEntry) -> bool {
        dir.depth() != 0 && self.file_filter(dir)
        // Don't send root
    }

    /// Determines if a directory should be traversed and caches the result
    #[inline]
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
        (self.file_filter)(&self.search_config, dir, self.custom_filter)
    }

    #[inline]
    fn parse_gitignore_file(base_dir: &Path) -> Option<Arc<Gitignore>> {
        let ignore_file = base_dir.join(".gitignore");
        let mut builder = GitignoreBuilder::new(base_dir);
        let _ = builder.add(ignore_file);

        builder.build().ok().map(Arc::new)
    }

    #[inline]
    fn build_ignore_context(
        &self,
        dir: &DirEntry,
        parent: Arc<IgnoreContext>,
    ) -> Arc<IgnoreContext> {
        debug_assert!(
            dir.is_traversible(),
            "we expect this entry to be traversible"
        );
        if !self.search_config.respect_gitignore {
            return parent;
        }

        let repo_active = parent.repo_active
            || dir
                .as_path()
                .join(".git")
                .symlink_metadata()
                .is_ok_and(|x| x.is_dir());
        let local_matcher = if repo_active {
            Self::parse_gitignore_file(dir.as_path())
        } else {
            None
        };

        IgnoreContext::child(parent, local_matcher, repo_active)
    }

    #[inline]
    fn is_gitignored(&self, dir: &DirEntry, ctx: &Arc<IgnoreContext>) -> bool {
        if !self.search_config.respect_gitignore || !ctx.repo_active {
            return false;
        }

        let mut current = Some(Arc::clone(ctx));
        let is_dir = self.should_traverse(dir);

        while let Some(node) = current.as_ref() {
            if let Some(matcher) = node.matcher.as_ref() {
                let matched = matcher.matched(dir.as_path(), is_dir);
                if matched.is_whitelist() {
                    return false;
                }
                if matched.is_ignore() {
                    return true;
                }
            }

            let next_parent = node.parent.clone();
            current.clone_from(&next_parent);
        }

        false
    }

    /**
    Advanced filtering for directories and symlinks with filesystem constraints.

    Handles same-filesystem constraints, inode caching, and symlink resolution
    to prevent infinite loops and duplicate traversal.
    */
    #[inline]
    #[expect(
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
    fn matches_ignore_path(&self, dir: &DirEntry) -> bool {
        self.search_config.matches_ignore_path(dir.as_bytes())
    }

    #[inline]
    fn handle_depth_limit(
        &self,
        dir: &DirEntry,
        should_send: bool,
        sender: &mut BatchSender,
        ctx: &WorkerContext<'_>,
    ) -> bool {
        if self
            .search_config
            .depth
            .is_some_and(|depth| dir.depth >= depth.get())
        {
            if should_send && sender.send(dir.clone()).is_err() {
                ctx.shutdown_flag.store(true, Ordering::Relaxed)
            } // Cloning costs very little here.
            return false; // Depth limit reached, stop processing
        }
        true // Continue processing
    }
    /**
    Recursively processes a directory, sending found files to a channel.

    This method uses a work-queue traversal with worker threads to process
    directories in parallel.

    */
    #[inline]
    fn process_directory(
        &self,
        work_item: WorkItem,
        sender: &mut BatchSender,
        ctx: &WorkerContext<'_>,
    ) {
        let WorkItem {
            dir,
            ignore_ctx: parent_ignore_ctx,
        } = work_item;

        if self.matches_ignore_path(&dir) || self.is_gitignored(&dir, &parent_ignore_ctx) {
            return;
        }

        if !self.directory_or_symlink_filter(&dir) {
            return; // Check for same filesystem/recursive symlinks etc, if so, return to avoid a loop/unnecessary info
        }

        let current_ignore_ctx = self.build_ignore_context(&dir, parent_ignore_ctx);

        let should_send_dir_or_symlink = self.should_send_dir(&dir); // If we've gotten here, the dir must be a directory!

        if !self.handle_depth_limit(&dir, should_send_dir_or_symlink, sender, ctx) {
            return;
        }
        // a macro to select the best implementation for your device (simplifying the code here)
        // On Linux/Android/Solaris/Illumos/etc, use getdents
        // on MacOS/FreeBSD, use getdirentries(64)
        // Otherwise use readdir
        match read_direntries!(dir) {
            Ok(entries) => {
                let (dirs, mut files): (Vec<_>, Vec<_>) = entries
                    // I'm not too happy with this method. need to revisit my approach.
                    .filter(|entry| {
                        self.keep_hidden(entry)
                            && !self.matches_ignore_path(entry)
                            && !self.is_gitignored(entry, &current_ignore_ctx)
                            && (self.should_traverse(entry) || self.file_filter(entry))
                    })
                    .partition(|ent| self.should_traverse(ent));

                for dirnt in dirs {
                    if !Self::enqueue_dir(dirnt, Arc::clone(&current_ignore_ctx), ctx) {
                        return;
                    }
                }

                // Checking if we should send directories
                if should_send_dir_or_symlink {
                    files.push(dir);
                }

                // We do batch sending to minimise contention of sending
                // as opposed to sending one at a time, which will cause tremendous locks
                for entry in files {
                    if sender.send(entry).is_err() {
                        ctx.shutdown_flag.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            }
            Err(error) => {
                if let Some(errors_arc) = self.errors.as_ref() {
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
    #[inline]
    fn enqueue_dir(dir: DirEntry, ignore_ctx: Arc<IgnoreContext>, ctx: &WorkerContext<'_>) -> bool {
        if ctx.shutdown_flag.load(Ordering::Relaxed) {
            // Release the shutdown as soon as possible.
            return false;
        }
        //atomicity itself ensures that all threads see a consistent modification order for pending,
        // so the final count will be correct even if increments are reordered among themselves.
        ctx.pending.fetch_add(1, Ordering::Relaxed);
        ctx.local.push(WorkItem { dir, ignore_ctx });

        true
    }
}
