//library imports
use rayon::prelude::*;
use std::{
    ffi::{OsStr, OsString},
    sync::mpsc::{channel as unbounded, Receiver, Sender},
    sync::Arc,
    //i use sync mpsc because it's faster than flume/crossbeam, didnt expect this!
};

//end library imports

//crate imports
mod iter;
pub(crate) use iter::DirIter;
mod test;
pub use direntry::LOCAL_PATH_MAX;
mod metadata;

mod dirent_macro;
mod direntry;
pub use direntry::DirEntry;

mod error;
pub use error::DirEntryError;

mod custom_types_result;
pub use custom_types_result::{OsBytes, Result};

mod traits_and_conversions;
pub use traits_and_conversions::{BytesToCstrPointer, PathToBytes, ToOsStr, ToStat};

mod utils;
pub use utils::{get_baselen, process_glob_regex, resolve_directory, unix_time_to_system_time};

pub(crate) use utils::strlen_asm;

mod glob;
pub use glob::glob_to_regex;
mod config;
pub use config::SearchConfig;
pub mod filetype;
pub use filetype::FileType;

//this allocator is more efficient than jemalloc through my testing
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug)]
/// A struct to find files in a directory.
pub struct Finder {
    root: OsString,
    search_config: SearchConfig,
    filter: Option<fn(&DirEntry) -> bool>,
}
///The Finder struct is used to find files in a directory.
impl Finder {
    #[must_use]
    #[allow(clippy::fn_params_excessive_bools)]
    #[allow(clippy::too_many_arguments)]
    //DUE TO INTENDED USAGE, THIS FUNCTION IS NOT TOO MANY ARGUMENTS.
    /// Create a new Finder instance.
    pub fn new(
        root: impl AsRef<OsStr>,
        pattern: impl AsRef<str>,
        hide_hidden: bool,
        case_insensitive: bool,
        keep_dirs: bool,
        short_path: bool,
        extension_match: Option<Arc<[u8]>>,
        max_depth: Option<u8>,
    ) -> Self {
        let config = SearchConfig::new(
            pattern,
            hide_hidden,
            case_insensitive,
            keep_dirs,
            short_path,
            extension_match,
            max_depth,
        );

        let search_config = match config {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Error creating search config: {e}");
                std::process::exit(1);
            }
        };

        Self {
            root: root.as_ref().to_owned(),
            search_config,
            filter: None,
        }
    }

    #[must_use]
    #[inline]
    /// Set a filter function to filter out entries.
    pub fn with_filter(mut self, filter: fn(&DirEntry) -> bool) -> Self {
        self.filter = Some(filter);
        self
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    /// Traverse the directory and return a receiver for the entries.
    pub fn traverse(&self) -> Result<Receiver<DirEntry>> {
        let (sender, receiver) = unbounded();

        let search_config = self.search_config.clone();

        let construct_dir = DirEntry::new(&self.root);

        if !construct_dir.as_ref().is_ok_and(DirEntry::is_dir) {
            return Err(DirEntryError::InvalidPath);
        }

        let filter = self.filter;

        //we have to arbitrarily construct a direntry to start the search.

        //spawn the search in a new thread.
        //this is safe because we've already checked that the directory exists.
        rayon::spawn(move || {
            Self::process_directory(
                unsafe { construct_dir.unwrap_unchecked() },
                &sender,
                &search_config,
                filter,
            );
        });

        Ok(receiver)
    }

    #[inline]
    /// Traverse the directory and send the `DirEntry` to the Receiver.
    fn process_directory(
        dir: DirEntry,
        sender: &Sender<DirEntry>,
        config: &SearchConfig,
        filter: Option<fn(&DirEntry) -> bool>,
    ) {
        //probably need to make a FSA to handle filter conditions

        // store whether we should send the directory itself
        let should_send = config.keep_dirs
            && config.matches_path(&dir, config.file_name)
            && filter.is_none_or(|f| f(&dir))
            && config.extension_match.as_ref().is_none() //no directories should match extensions (mostly? not sure.)
            && dir.depth() !=0; //dont send the root directory

        //check if we should stop searching
        if config.depth.is_some_and(|d| dir.depth() >= d) {
            if should_send {
                let _ = sender.send(dir);
            }
            return;
        }
        //match dir.as_iter()  example of how to use the iterator
        match dir.read_dir() {
            Ok(entries) => {
                let mut dirs = Vec::new(); //maybe smallvec here.

                for entry in entries {
                    if config.hide_hidden && entry.is_hidden() {
                        continue;
                    }

                    if entry.is_dir() {
                        // always include directories for traversal
                        dirs.push(entry);
                    } else if filter.is_none_or(|f| f(&entry))
                        && config.matches_path(&entry, config.file_name)
                        && config
                            .extension_match
                            .as_ref()
                            .is_none_or(|ext| entry.matches_extension(ext))
                    {
                        // only filter non-directory entries
                        let _ = sender.send(entry);
                        //the only error that should happen here is if the receiver is dropped, which is fine.
                        //this only happens when we cut the receiver due to limiting entries,
                    }
                }

                dirs.into_par_iter().for_each(|dir| {
                    Self::process_directory(dir, sender, config, filter);
                });
            }

            Err(DirEntryError::AccessDenied(_) | DirEntryError::InvalidPath) => {
                // ignore permission denied and invalid path errors
                //these will happen if the directory is not accessible(eg /etc/)
                //or the path changes midway throughout processing, like /proc/
                //this is a common error, so we can ignore it.
            }
            //enoent= no such file or directory
            //eacces=permission denied
            //enotdir=not a directory
            //eloop=too many symbolic links
            Err(e) => {
                eprintln!("Unexpected error: {e}"); //i need to polish this up a bit fuck off.
            }
        }
        //finally send it
        //this needs to be done at the end because it consumes the dir.
        if should_send {
            let _ = sender.send(dir);
        }
    }
}
