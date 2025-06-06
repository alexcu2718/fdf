//library imports
#![allow(clippy::single_call_fn)]
#![allow(clippy::let_underscore_must_use)]
#![allow(clippy::let_underscore_untyped)]
#![allow(clippy::macro_metavars_in_unsafe)]
#![allow(clippy::shadow_unrelated)]
#![allow(clippy::print_stderr)]
#![allow(clippy::implicit_return)]
#![allow(clippy::as_underscore)]
#![allow(clippy::print_stderr)]
#![allow(clippy::min_ident_chars)]
#![allow(clippy::implicit_return)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(clippy::undocumented_unsafe_blocks)]
#![allow(clippy::blanket_clippy_restriction_lints)]
#![allow(clippy::absolute_paths)]
#![allow(clippy::impl_trait_in_params)]
#![allow(clippy::arbitrary_source_item_ordering)]
#![allow(clippy::std_instead_of_core)]
#![allow(clippy::filetype_is_file)]
#![allow(clippy::missing_assert_message)]
#![allow(clippy::unused_trait_names)]
#![allow(clippy::exhaustive_enums)]
#![allow(clippy::exhaustive_structs)]
#![allow(clippy::missing_inline_in_public_items)]
#![allow(clippy::std_instead_of_alloc)]
#![allow(clippy::unseparated_literal_suffix)]
#![allow(clippy::pub_use)]
#![allow(clippy::field_scoped_visibility_modifiers)]
#![allow(clippy::pub_with_shorthand)]
#![allow(clippy::redundant_pub_crate)]
#![allow(clippy::allow_attributes)]
#![allow(clippy::allow_attributes_without_reason)]
#![allow(clippy::use_debug)]
#![allow(clippy::map_err_ignore)]
#![allow(clippy::exit)]
#![allow(clippy::cast_ptr_alignment)]
#![allow(clippy::multiple_unsafe_ops_per_block)]
#![allow(clippy::pattern_type_mismatch)]
#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::as_conversions)]
#![allow(clippy::question_mark_used)]
#![allow(clippy::semicolon_if_nothing_returned)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::missing_trait_methods)]
#![allow(clippy::default_numeric_fallback)]
#![allow(clippy::wildcard_enum_match_arm)]
#![allow(clippy::semicolon_inside_block)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::semicolon_outside_block)]
#![allow(clippy::return_and_then)]
#![allow(clippy::cast_possible_wrap)]
//#![allow(clippy::non_ex)]
use rayon::prelude::*;
use std::{
    ffi::{OsStr, OsString},
    sync::Arc,
    //i use sync mpsc because it's faster than flume/crossbeam, didnt expect this!
    sync::mpsc::{Receiver, Sender, channel as unbounded},
};

mod dirent_macro;
//
//pub(crate) use dirent_macro::construct_path;

//end library imports

//crate imports
mod iter;
pub(crate) use iter::DirIter;

mod test;

mod metadata;

mod buffer;
pub(crate) use buffer::AlignedBuffer;

mod direntry;
pub use direntry::DirEntry;

mod error;
pub use error::DirEntryError;

mod custom_types_result;
pub use custom_types_result::{
    AsU8, BUFFER_SIZE, LOCAL_PATH_MAX, OsBytes, PathBuffer, Result, SyscallBuffer,
};

mod traits_and_conversions;
pub use traits_and_conversions::{AsOsStr, BytesToCstrPointer, PathAsBytes, ToStat};

mod utils;

pub(crate) use utils::strlen_asm;
pub use utils::unix_time_to_system_time;
mod glob;
pub use glob::glob_to_regex;
mod config;
pub use config::SearchConfig;
pub mod filetype;
pub use filetype::FileType;

//this allocator is more efficient than jemalloc through my testing(still better than system allocator)
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
    #[inline]
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
    fn process_directory(
        dir: DirEntry,
        sender: &Sender<DirEntry>,
        config: &SearchConfig,
        filter: Option<fn(&DirEntry) -> bool>,
    ) {
        let should_send = config.keep_dirs
            && config.matches_path(&dir, config.file_name)
            && filter.is_none_or(|f| f(&dir))
            && config.extension_match.as_ref().is_none()
            && dir.depth() != 0;

        if should_send && config.depth.is_some_and(|d| dir.depth() >= d) {
            let _ = sender.send(dir);

            return; // stop processing this directory if depth limit is reached
        }

        match dir.getdents() {
            Ok(entries) => {
                // Store only directories for parallel recursive call
                let mut dirs = Vec::new();

                for entry in entries.filter(|e| !config.hide_hidden || !e.is_hidden()) {
                    if entry.is_dir() {
                        dirs.push(entry); // save dir for parallel traversal
                    } else {
                        // apply filters and send files immediately
                        if filter.is_none_or(|f| f(&entry))
                            && config.matches_path(&entry, config.file_name)
                            && config
                                .extension_match
                                .as_ref()
                                .is_none_or(|ext| entry.matches_extension(ext))
                        {
                            let _ = sender.send(entry);
                        }
                    }
                }

                // send into  directories in parallel (via vec) which is threadsafe, we could ideally switch storage type to arc and then
                // use rayon::iter::IntoParallelIterator for more efficient parallel processing , ill try that in an experimental build.
                dirs.into_par_iter().for_each(|dir| {
                    Self::process_directory(dir, sender, config, filter);
                });
            }
            Err(DirEntryError::AccessDenied(_) | DirEntryError::InvalidPath) => {}
            Err(e) => eprintln!("Unexpected error: {e}"),
        }

        if should_send {
            let _ = sender.send(dir);
        }
    }
}
