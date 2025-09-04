use rayon::prelude::*;
use std::{
    ffi::{OsStr, OsString},
    sync::mpsc::{Receiver, Sender, channel as unbounded},
};
#[macro_use]
pub(crate) mod cursed_macros;

pub mod size_filter;

use size_filter::SizeFilter;
pub mod printer;
mod temp_dirent;
#[cfg(target_os = "linux")]
pub use temp_dirent::TempDirent;

mod iter;
pub(crate) use iter::DirIter;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod direntry_filter;

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

pub use custom_types_result::{
    BUFFER_SIZE, BytesStorage, DirEntryFilter, FilterType, LOCAL_PATH_MAX, OsBytes, Result,
    SlimmerBytes,
};

pub(crate) use custom_types_result::PathBuffer;
#[cfg(target_os = "linux")]
pub(crate) use custom_types_result::SyscallBuffer;

mod traits_and_conversions;
pub(crate) use traits_and_conversions::BytePath;

mod utils;
#[cfg(any(target_os = "linux", target_os = "illumos", target_os = "solaris"))]
pub use utils::dirent_const_time_strlen;
pub use utils::{strlen, unix_time_to_datetime};

mod glob;
pub use glob::glob_to_regex;
mod config;
pub use config::SearchConfig;
mod filetype;
pub use filetype::FileType;

//this allocator is more efficient than jemalloc through my testing(still better than system allocator)
#[cfg(all(any(target_os = "linux", target_os = "macos"), not(miri)))]
//miri doesnt support custom allocators
//not sure which platforms support this, BSD doesnt from testing, will test others as appropriate(GREAT DOCS!!!)
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

// The `Finder` struct is the main entry point for the file search.
// Its methods are exposed for building the search configuration
#[derive(Debug)]
/// A struct to find files in a directory.
///
/// This is the core component of the library. It is responsible for
/// configuring and executing the parallel directory traversal
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
    #[allow(clippy::missing_errors_doc)]
    /// Traverse the directory and return a receiver for the entries.
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
            Err(DirEntryError::InvalidPath)
        }
    }

    #[inline]
    /// Recursively processes a directory, sending found files to a channel.
    ///
    /// This method uses a depth-first traversal with `rayon` to process directories
    /// in parallel.
    ///
    /// # Arguments
    /// * `dir` - The `DirEntry` representing the directory to process.
    /// * `sender` - A channel `Sender` to send batches of found `DirEntry`s.
    fn process_directory(&self, dir: DirEntry<S>, sender: &Sender<Vec<DirEntry<S>>>) {
        let config = &self.search_config;
        //the filter for keeping files/dirs (as appropriate), this could be a function TODO-MAYBE
        let file_filter = |file_entry: &DirEntry<S>| -> bool {
            (self.custom_filter)(config, file_entry, self.filter)
        };

        let should_send_dir_or_symlink =//CHECK IF WE SHOULD SEND DIRS
            config.keep_dirs && file_filter(&dir) && dir.depth() != 0; //dont send root directory

        if config.depth.is_some_and(|d| dir.depth >= d) {
            if should_send_dir_or_symlink {
                let _ = sender.send(vec![dir]);
            } //have to put into a vec, this doesnt matter because this only happens when we depth limit
            //i purposely ignore the result here because of pipe errors/other errors (like  -n 10 ) that i should probably log.
            //TODO-MAYBE

            return; // stop processing this directory if depth limit is reached
        }
        #[cfg(target_os = "linux")]
        //linux with getdents (only linux has stable ABI, so we can lower down to assembly here, not for any other system tho)
        let direntries = dir.getdents(); //additionally, readdir internally calls stat on each file, which is expensive and unnecessary from testing!
        #[cfg(not(target_os = "linux"))]
        let direntries = dir.readdir(); //in theory i can use getattrlistbulk on macos, this has  a LOT of complexity!
        //https://man.freebsd.org/cgi/man.cgi?query=getattrlistbulk&sektion=2&manpath=macOS+13.6.5 TODO!
        // TODO! FIX THIS SEPARATE REPO https://github.com/alexcu2718/mac_os_getattrlistbulk_ls
        // THIS REQUIRES A LOT MORE WORK TO VALIDATE SAFETY BEFORE I CAN USE IT, IT'S ALSO VERY ROUGH SINCE MACOS API IS TERRIBLE

        //these lambdas make the code a bit easier to follow, i might define them as functions later, TODO-MAYBE!
        //directory or symlink lambda
        // Keep all directories (and symlinks if following them)
        let d_or_s_filter = |myentry: &DirEntry<S>| -> bool {
            myentry.is_dir() || config.follow_symlinks && myentry.is_symlink()
        };
        let keep_hidden =
            |hfile: &DirEntry<S>| -> bool { !config.hide_hidden || !hfile.is_hidden() };

        match direntries {
            Ok(entries) => {
                // This boolean logic is designed for efficiency through short-circuiting.
                // 1. We first check `keep_hidden`. If a file is hidden and `hide_hidden` is true,
                //    the entire expression immediately evaluates to `false`, and we move to the next entry.
                // 2. If the entry is not hidden (or `hide_hidden` is false), we then check
                //    `(d_or_s_filter(entry) || file_filter(entry))`.
                // 3. This part uses another short-circuit. `d_or_s_filter` checks if the entry is
                //    a directory or a symlink we should follow. If it is, the right side (`file_filter`)
                //    is not evaluated, which avoids an expensive call to `file_filter` on directories.
                // 4. If the entry is not a directory/symlink, we then run the `file_filter`, which
                //    contains the main logic for filtering files (e.g., by name, size, filetype etc).
            let (dirs, mut files): (Vec<_>, Vec<_>) = entries
                .filter(|entry| keep_hidden(entry) &&
                (d_or_s_filter(entry) || file_filter(entry)))
                .partition(d_or_s_filter);

                // Process directories in parallel
                dirs.into_par_iter().for_each(|dirnt| {
                    self.process_directory( dirnt, sender);
                });
                //checking if we should send directories
                if should_send_dir_or_symlink{
                    #[allow(clippy::redundant_clone)] //we have to clone here at onne point, compiler doesnt like it because we're not using the result
                    files.push(dir.clone()) //we have to clone here unfortunately because it's being used to keep the iterator going.
                    //luckily we're only cloning 1 directory/symlink, not anything more than that.
                }
                //We do batch sending to minimise contention of sending 
                //as opposed to sending one at a time, which will cause tremendous locks
                if !files.is_empty() {
                    let _ = sender.send(files);
                }
            }
            Err(
                DirEntryError::TemporarilyUnavailable // can possibly get rid of this
                | DirEntryError::AccessDenied(_) //this will occur, i should probably provide an option to  display errors TODO!
                | DirEntryError::InvalidPath, //naturally this will happen  due to  quirks like seen in /proc
            ) => {} //TODO! add logging
            #[allow(clippy::used_underscore_binding)]
            Err(_err) => {
            #[allow(clippy::panic)] //panic is only in debug. This should trigger any CI warnings i am using!
            #[cfg(debug_assertions)]
            {
                // In debug mode, show the error and panic. this is extremely helpful for debugging potential issues
                panic!("Unreachable directory entry error: {_err:?}");
            }
            // In release mode, the compiler will use unreachable_unchecked().
            // This provides a performance optimisation by assuming this code path is never taken.
            // Safety: we assume all other error variants are covered and this is indeed unreachable.
            #[cfg(not(debug_assertions))]
            unsafe {
                core::hint::unreachable_unchecked();
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
#[allow(clippy::struct_excessive_bools)] //NATURALLY ANY BUILDER WILL CONTAIN A LOT OF BOOLS FFS
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
    pub(crate) file_type: Option<u8>, //u8= a length one byte string , aka b"f" or b"s"
}

impl<S> FinderBuilder<S>
where
    S: BytesStorage + 'static + Clone + Send,
{
    /// Create a new `FinderBuilder` with required fields
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
    /// Set whether to use short pathss in regex matching, defaults to true
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
    /// Set maximum search depth
    pub const fn filter_by_size(mut self, size_of: Option<SizeFilter>) -> Self {
        self.size_filter = size_of;
        self
    }

    /// Set whether to follow symlinks, defaults to false. Careful for recursion!
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
    pub const fn type_filter(mut self, filter: Option<u8>) -> Self {
        self.file_type = filter;
        self
    }

    /// Build the Finder instance
    #[allow(clippy::missing_errors_doc)] //TODO! add error docs here
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
        )?;

        let lambda: FilterType<S> = |rconfig, rdir, rfilter| {
            {
                rfilter.is_none_or(|f| f(rdir))
                    && rconfig.matches_type(rdir)
                    && rconfig.matches_extension(&rdir.file_name())
                    && rconfig.matches_size(rdir)
                    && rconfig.matches_path(rdir, rconfig.file_name_only)
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
