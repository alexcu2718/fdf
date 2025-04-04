#![allow(clippy::inline_always)]
#![feature(anonymous_pipe)]
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
mod direntry_tests;

mod metadata;

mod dirent_macro;
mod direntry;
pub use direntry::DirEntry;

mod error;
pub use error::DirEntryError;

mod custom_types_result;
pub use custom_types_result::{OsBytes, Result};

mod traits_and_conversions;
pub(crate) use traits_and_conversions::ToStat;
pub use traits_and_conversions::{BytesToCstrPointer, PathToBytes, ToOsStr};

mod utils;
pub use utils::{get_baselen, process_glob_regex, resolve_directory, unix_time_to_system_time};

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
    //luckily avoid making it dyn, as we can just use a function pointer.
    //this is because we can't use a trait object here, as we need to be able to clone the Finder struct.
    //and we can't clone a trait object.
    //so we use a function pointer instead.
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
        pattern: &str,
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
                true,
            );
        });

        Ok(receiver)
    }

    #[inline]
    #[allow(clippy::unnecessary_map_or)]
    //i use map_or because compatibility with 1.74 as is_none_or is unstable until 1.82(ish)
    /// Traverse the directory and send the `DirEntry` to the Receiver.
    fn process_directory(
        dir: DirEntry,
        sender: &Sender<DirEntry>,
        config: &SearchConfig,
        filter: Option<fn(&DirEntry) -> bool>,
        is_start_dir: bool,
    ) {
        // store whether we should send the directory itself
        let should_send = config.keep_dirs
            && config.matches_path(&dir, config.file_name)
            && filter.map_or(true, |f| f(&dir))
            && config.extension_match.as_ref().is_none() //no directories should match extensions (mostly? not sure.)
            && !is_start_dir;

        //check if we should stop searching
        if config.depth.is_some_and(|d| dir.depth() >= d) {
            if should_send {
                let _ = sender.send(dir);
            }
            return;
        }
        //match dir.as_iter()  example of how to use the iterator
        match DirEntry::read_dir(&dir) {
            Ok(entries) => {
                let mut dirs = Vec::with_capacity(entries.len() / 2);

                for entry in entries {
                    if config.hide_hidden && entry.is_hidden() {
                        continue;
                    }

                    if entry.is_dir() {
                        // always include directories for traversal
                        dirs.push(entry);
                    } else if filter.map_or(true, |f| f(&entry))
                        && config.matches_path(&entry, config.file_name)
                        && config
                            .extension_match
                            .as_ref()
                            .map_or(true, |ext| entry.matches_extension(ext))
                    {
                        // only filter non-directory entries
                        let _ = sender.send(entry);
                        //the only error that should happen here is if the receiver is dropped, which is fine.
                        //this only happens when we cut the receiver due to limiting entries,
                    }
                }

                dirs.into_par_iter().for_each(|dir| {
                    Self::process_directory(dir, sender, config, filter, false);
                });
            }

            Err(DirEntryError::AccessDenied(_) | DirEntryError::InvalidPath) => {
                // ignore permission denied and invalid path errors
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
