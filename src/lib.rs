#![allow(clippy::inline_always)]

//use std::sync::Arc;
//use std::intrinsics::likely;

//library imports
use libc::{EACCES, EINVAL, ELOOP, ENOENT, ENOTDIR};
use rayon::prelude::*;
use std::{
    ffi::OsString,
    sync::mpsc::{channel as unbounded, Receiver, Sender},
    sync::Arc,
    //i use sync mpsc because it's faster than flume/crossbeam, didnt expect this!
};

//end library imports

//crate imports

mod direntry;
pub use direntry::DirEntry;

mod pointer_conversion;
pub use pointer_conversion::PointerUtils;
mod utils;
pub use utils::{ process_glob_regex, resolve_directory};
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
        root: OsString,
        pattern: &str,
        hide_hidden: bool,
        case_insensitive: bool,
        keep_dirs: bool,
        short_path: bool,
        extension_match: Option<Arc<[u8]>>,
        max_depth: Option<u8>,
    ) -> Self {
        let search_config = SearchConfig::new(
            pattern,
            hide_hidden,
            case_insensitive,
            keep_dirs,
            short_path,
            extension_match,
            max_depth,
        );

        Self {
            root,
            search_config,
            filter: None,
        }
    }

    #[must_use]
    #[inline(always)]
    /// Set a filter function to filter out entries.
    pub fn with_filter(mut self, filter: fn(&DirEntry) -> bool) -> Self {
        self.filter = Some(filter);
        self
    }

    #[must_use]
    #[inline(always)]
    /// Traverse the directory and return a receiver for the entries.
    pub fn traverse(&self) -> Receiver<DirEntry> {
        let (sender, receiver) = unbounded();

        let search_config = self.search_config.clone();

        let construct_dir = DirEntry::new(&self.root);

        if !construct_dir.is_dir() {
            eprintln!("Error: The provided path is not a directory.");
            std::process::exit(1);
        }




        let filter = self.filter;

        //we have to arbitrarily construct a direntry to start the search.

        rayon::spawn(move || {
            Self::process_directory(construct_dir, &sender, &search_config, filter, true);
        });

        receiver
    }

    #[inline(always)]
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
            && filter.as_ref().map_or(true, |f| f(&dir))
            && config.extension_match.as_ref().is_none()
            && !is_start_dir;

        //check if we should stop searching
        if config.depth.is_some_and(|d| dir.depth() >= d) {
            if should_send {
                let _ = sender.send(dir);
            }
            return;
        }

        match DirEntry::read_dir(&dir) {
            Ok(entries) => {
                let mut dirs = Vec::with_capacity(entries.len());

                for entry in entries {
                    if config.hide_hidden && entry.is_hidden() {
                        continue;
                    }

                    if entry.is_dir() {
                        // always include directories for traversal
                        dirs.push(entry);
                    } else if filter.as_ref().map_or(true, |f| f(&entry))
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

            Err(e)
                if matches!(
                    e.raw_os_error(),
                    Some(EINVAL | ENOENT | EACCES | ENOTDIR | ELOOP) //einval=invalid argument
                                                                     //enoent= no such file or directory
                                                                     //eacces=permission denied
                                                                     //enotdir=not a directory
                                                                     //eloop=too many symbolic links
                ) => {}
            Err(check) => {
                eprintln!("this is a new error i havent seen LOL {check}");
                //this is for debugging purposes, because i still dont know what other errors exist.
            }
        }
        //finally send it
        //this needs to be done at the end because it consumes the dir.
        if should_send {
            let _ = sender.send(dir);
        }
    }
}
