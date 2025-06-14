//library imports
#![allow(clippy::single_call_fn)]
#![allow(clippy::let_underscore_must_use)]
#![allow(clippy::let_underscore_untyped)]
#![allow(clippy::macro_metavars_in_unsafe)]
#![allow(clippy::shadow_unrelated)]
#![allow(clippy::print_stderr)]
#![allow(clippy::implicit_return)]
#![allow(clippy::doc_lazy_continuation)]
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

//crate imports
mod iter;
pub(crate) use iter::DirIter;

mod direntry_filter;
pub use direntry_filter::DirEntryIteratorFilter;

mod buffer;
mod test;
pub(crate) use buffer::AlignedBuffer;

mod direntry;
pub use direntry::DirEntry;

mod error;
pub use error::DirEntryError;

mod custom_types_result;
pub use custom_types_result::{
    AsU8, BUFFER_SIZE, BytesStorage, DirEntryFilter, FilterType, LOCAL_PATH_MAX, OsBytes,
    PathBuffer, Result, SlimmerBytes, SyscallBuffer,
};

mod traits_and_conversions;
pub use traits_and_conversions::{BytePath, PathAsBytes};

mod utils;

//pub(crate) use utils::strlen_asm;
pub use utils::{dirent_const_time_strlen, unix_time_to_system_time};

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
pub struct Finder<S>
where
    S: BytesStorage,
{
    root: OsString,
    search_config: SearchConfig,
    filter: Option<DirEntryFilter<S>>,
    custom_filter: FilterType<S>,
}
///The Finder struct is used to find files in a directory.
impl<S> Finder<S>
//S is a generic type that implements BytesStorage trait aka  vec/arc/box/slimmerbox(alias to SlimmerBytes)
where
    S: BytesStorage + 'static + Clone + Send,
{
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
        // The lambda functions are used to filter directories and non-directories based on the search configuration.
        let lambda: FilterType<S> = |rconfig, rdir, rfilter| {
            {
                rfilter.is_none_or(|f| f(rdir))
                    && rconfig.matches_path(rdir, rconfig.file_name)
                    && rconfig
                        .extension_match
                        .as_ref() //get the filename THEN check extension, we dont want to pick up
                        //stuff like .gitignore or .DS_Store
                        .is_none_or(|ext| {
                            (&rdir.as_bytes()[rdir.base_len()..]).matches_extension(ext)
                        })
            }
        };

        Self {
            root: root.as_ref().to_owned(),
            search_config,
            filter: None,
            custom_filter: lambda,
        }
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

        //we have to arbitrarily construct a direntry to start the search.
        let construct_dir = DirEntry::new(&self.root);

        if !construct_dir.as_ref().is_ok_and(DirEntry::is_dir) {
            return Err(DirEntryError::InvalidPath);
        }

        //spawn the search in a new thread.
        //this is safe because we've already checked that the directory exists.
        rayon::spawn(move || {
            Self::process_directory(&self, unsafe { construct_dir.unwrap_unchecked() }, &sender);
        });

        Ok(receiver)
    }

    #[inline]
    #[allow(clippy::redundant_clone)] //we have to clone here at onne point, compiler doesnt like it because we're not using the result
    fn process_directory(&self, dir: DirEntry<S>, sender: &Sender<Vec<DirEntry<S>>>) {
        let should_send = self.search_config.keep_dirs
            && (self.custom_filter)(&self.search_config, &dir, self.filter)
            && dir.depth() != 0;

        if should_send && self.search_config.depth.is_some_and(|d| dir.depth >= d) {
            let _ = sender.send(vec![dir]); //have to put into a vec, this doesnt matter because this only happens when we depth limit

            return; // stop processing this directory if depth limit is reached
        }

        match dir.getdents() {
            Ok(entries) => {
                // Store only directories for parallel recursive call

                let (dirs, files): (Vec<_>, Vec<_>) = entries
                    .filter(|e| !self.search_config.hide_hidden || !e.is_hidden())
                    .partition(direntry::DirEntry::is_dir);

                dirs.into_par_iter().for_each(|dir| {
                    Self::process_directory(self, dir, sender);
                });

                // Process files without intermediate Vec
                let matched_files: Vec<_> = files
                    .into_iter()
                    .filter(|entry| (self.custom_filter)(&self.search_config, entry, self.filter))
                    .chain(should_send.then(|| dir.clone())) // Include `dir` if `should_send`, we have to clone it unfortunately
                    .collect(); //by doing it this way we reduce channel contention and avoid an intermediate vec, which is more efficient!

                if !matched_files.is_empty() {
                    let _ = sender.send(matched_files);
                }
            }
            Err(
                DirEntryError::Success
                | DirEntryError::TemporarilyUnavailable
                | DirEntryError::AccessDenied(_)
                | DirEntryError::InvalidPath,
            ) => {}
            Err(e) => eprintln!("Unexpected error: {e}"),
        }
    }
}
