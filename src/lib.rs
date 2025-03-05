use libc::{EACCES, EINVAL, ELOOP, ENOENT, ENOTDIR};
use rayon::prelude::*;

mod direntry;
pub use direntry::DirEntry;
mod utils;
pub use utils::{escape_regex_string, get_depth, process_glob_regex, read_dir, resolve_directory};
mod config;
pub use config::SearchConfig;

use std::{
    ffi::OsString,
    os::unix::ffi::OsStrExt,
    sync::mpsc::{channel as unbounded, Receiver, Sender},
    //i use sync mpsc because it's faster than flume/crossbeam, didnt expect this!
};

//this allocator is more efficient than jemalloc through my testing
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Clone)]
pub struct Finder {
    root: OsString,
    search_config: SearchConfig,
    filter: Option<fn(&DirEntry) -> bool>,
    //luckily avoid making it dyn, as we can just use a function pointer.
    //this is because we can't use a trait object here, as we need to be able to clone the Finder struct.
    //and we can't clone a trait object.
    //so we use a function pointer instead.
    //this is a bit of a hack, but it works.
}

impl Finder {
    #[must_use]
    #[allow(clippy::fn_params_excessive_bools)]
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub fn new(
        root: OsString,
        pattern: &str,
        hide_hidden: bool,
        case_insensitive: bool,
        keep_dirs: bool,
        short_path: bool,
        extension_match: Option<Box<[u8]>>,
    ) -> Self {
        let search_config = SearchConfig::new(
            pattern,
            hide_hidden,
            case_insensitive,
            keep_dirs,
            short_path,
            extension_match,
        );
        Self {
            root,
            search_config,
            filter: None,
        }
    }

    #[must_use]
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub fn with_filter(mut self, filter: fn(&DirEntry) -> bool) -> Self {
        self.filter = Some(filter);
        self
    }

    #[must_use]
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub fn traverse(&self) -> Receiver<DirEntry> {
        let (sender, receiver) = unbounded();

        let search_config = self.search_config.clone();

        let construct_dir = DirEntry {
            path: self.root.as_bytes().into(),
            is_dir: true,
            is_unknown: false,     //cheap check only initialised once.
            is_regular_file: true, //vague assumptions that i cant be bothered to check.
            is_fifo: false,
            is_block: false,
            is_char: false,
            is_socket: false,
        };

        let filter = self.filter;
        //we have to arbitrarily construct a direntry to start the search.

        rayon::spawn(move || {
            Self::process_directory(construct_dir, &sender, &search_config, filter);
        });

        receiver
    }

    #[inline(always)]
    #[allow(clippy::unnecessary_map_or)]
    //i use map_or because compatibility with 1.74 as is_none_or is unstable until 1.82(ish)
    #[allow(clippy::inline_always)]
    fn process_directory(
        dir: DirEntry,
        sender: &Sender<DirEntry>,
        config: &SearchConfig,
        filter: Option<fn(&DirEntry) -> bool>,
    ) {
        // store whether we should send the directory itself
        let should_send = config.keep_dirs
            && config.matches_path(&dir.path)
            && filter.as_ref().map_or(true, |f| f(&dir))
            && config.extension_match.as_ref().is_none(); //map_or(true, |ext| dir.matches_extension(&ext));
        match DirEntry::new(&dir.path) {
            Ok(entries) => {
                let mut dirs = Vec::with_capacity(16);

                for entry in entries {
                    if config.hide_hidden && entry.is_hidden() {
                        continue;
                    }

                    if entry.is_dir {
                        // always include directories for traversal
                        dirs.push(entry);
                    } else if filter.as_ref().map_or(true, |f| f(&entry))
                        && config.matches_path(&entry.path)
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
                    Self::process_directory(dir, sender, config, filter);
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
