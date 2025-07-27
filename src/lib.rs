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
#![allow(clippy::cast_lossless)]
//#![allow(clippy::non_ex)]

use rayon::prelude::*;
use std::{
    ffi::{OsStr, OsString},
    //i use sync mpsc because it's faster than flume/crossbeam, didnt expect this!
    sync::mpsc::{Receiver, Sender, channel as unbounded},
};
#[macro_use]
pub(crate) mod cursed_macros;

mod temp_dirent;
pub use temp_dirent::TempDirent;
//crate imports
mod iter;
pub(crate) use iter::DirIter;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod direntry_filter;


#[cfg(target_os = "linux")]
mod syscalls;
#[cfg(target_os = "linux")]
pub use syscalls::{open_asm,close_asm,getdents_asm};

mod buffer;
mod test;
pub use buffer::AlignedBuffer;

mod memchr_derivations;
pub use memchr_derivations::{contains_zero_byte, find_zero_byte_u64, memrchr};
mod direntry;
pub use direntry::DirEntry;

mod error;
pub use error::DirEntryError;

mod custom_types_result;

pub use custom_types_result::{
    BUFFER_SIZE, BytesStorage, DirEntryFilter, FilterType, LOCAL_PATH_MAX, OsBytes, Result,
    SlimmerBytes,
};

pub(crate) use custom_types_result::{PathBuffer, SyscallBuffer};

mod traits_and_conversions;
pub use traits_and_conversions::BytePath;

mod utils;

//pub(crate) use utils::strlen_asm;
#[cfg(target_os = "linux")]
pub use utils::dirent_const_time_strlen;
pub use utils::{strlen, unix_time_to_system_time};

mod glob;
pub use glob::glob_to_regex;
mod config;
pub use config::SearchConfig;
mod filetype;
pub use filetype::FileType;

//this allocator is more efficient than jemalloc through my testing(still better than system allocator)
#[cfg(any(target_os = "linux", target_os = "macos"))]
//not sure which platforms support this, BSD doesnt from testing, will test others as appropriate(GREAT DOCS!!!)
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug)]
/// A struct to find files in a directory.
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
    pub fn init(root: impl AsRef<OsStr>, pattern: impl AsRef<str>) -> FinderBuilder<S> {
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

        // we have to arbitrarily construct a direntry to start the search.
        let construct_dir = DirEntry::new(&self.root);
        // check if the directory exists and is traversible
        // traversible meaning directory/symlink, we follow symlinks as it's the starting filepath
        // but henceforth we do not follow symlinks unless specified in the config
        // this is to prevent infinite loops and other issues.
        match construct_dir {
            Ok(entry) if entry.is_traversible() => {
                // spawn the search in a new thread.
                rayon::spawn(move || {
                    Self::process_directory(&self, entry, &sender);
                });

                //return receiver
                Ok(receiver)
            }
            _ => Err(DirEntryError::InvalidPath),
        }
    }
    //linux with getdents (only linux has stable ABI, so we can lower down to assembly here, not for any other system tho)
    #[inline]
    #[cfg(target_os = "linux")]
    #[allow(clippy::redundant_clone)] //we have to clone here at onne point, compiler doesnt like it because we're not using the result
    fn process_directory(&self, dir: DirEntry<S>, sender: &Sender<Vec<DirEntry<S>>>) {
        let config = &self.search_config;

        let should_send =
            config.keep_dirs && (self.custom_filter)(config, &dir, self.filter) && dir.depth() != 0;

        if self.search_config.depth.is_some_and(|d| dir.depth >= d) {
            if should_send {
                let _ = sender.send(vec![dir]);
            } //have to put into a vec, this doesnt matter because this only happens when we depth limit

            return; // stop processing this directory if depth limit is reached
        }

        match dir.getdents() {
            Ok(entries) => {
                // Store only directories for parallel recursive call

                let (dirs, files): (Vec<_>, Vec<_>) = entries
                    .filter(|e| !config.hide_hidden || !e.is_hidden())
                    .partition(|x| x.is_dir() || config.follow_symlinks && x.is_symlink());

                dirs.into_par_iter().for_each(|dir| {
                    Self::process_directory(self, dir, sender);
                });

                // Process files without intermediate Vec
                let matched_files: Vec<_> = files
                    .into_iter()
                    .filter(|entry| (self.custom_filter)(config, entry, self.filter))
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
    //non linux version
    #[inline]
    #[cfg(not(target_os = "linux"))]
    #[allow(clippy::redundant_clone)] //we have to clone here at onne point, compiler doesnt like it because we're not using the result
    fn process_directory(&self, dir: DirEntry<S>, sender: &Sender<Vec<DirEntry<S>>>) {
        let config = &self.search_config;

        let should_send =
            config.keep_dirs && (self.custom_filter)(config, &dir, self.filter) && dir.depth() != 0;

        if self.search_config.depth.is_some_and(|d| dir.depth >= d) {
            if should_send {
                let _ = sender.send(vec![dir]);
            } //have to put into a vec, this doesnt matter because this only happens when we depth limit

            return; // stop processing this directory if depth limit is reached
        }

        match dir.readdir() {
            Ok(entries) => {
                // Store only directories for parallel recursive call

                let (dirs, files): (Vec<_>, Vec<_>) = entries
                    .filter(|e| !config.hide_hidden || !e.is_hidden())
                    .partition(|x| x.is_dir() || config.follow_symlinks && x.is_symlink());

                dirs.into_par_iter().for_each(|dir| {
                    Self::process_directory(self, dir, sender);
                });

                // Process files without intermediate Vec
                let matched_files: Vec<_> = files
                    .into_iter()
                    .filter(|entry| (self.custom_filter)(config, entry, self.filter))
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

/// A builder for creating a `Finder` instance with customisable options.
///
/// This builder allows you to set various options such as hiding hidden files, case sensitivity,
/// keeping directories in results, matching file extensions, setting maximum search depth,
/// following symlinks, and applying a custom filter function.
#[allow(clippy::struct_excessive_bools)] //....
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
    pub(crate) max_depth: Option<u8>,
    pub(crate) follow_symlinks: bool,
    pub(crate) filter: Option<DirEntryFilter<S>>,
}

impl<S> FinderBuilder<S>
where
    S: BytesStorage + 'static + Clone + Send,
{
    /// Create a new `FinderBuilder` with required fields
    pub fn new(root: impl AsRef<OsStr>, pattern: impl AsRef<str>) -> Self {
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
    pub const fn max_depth(mut self, max_depth: Option<u8>) -> Self {
        self.max_depth = max_depth;
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

    /// Build the Finder instance
    pub fn build(self) -> Finder<S> {
        let config = SearchConfig::new(
            self.pattern,
            self.hide_hidden,
            self.case_insensitive,
            self.keep_dirs,
            self.file_name_only,
            self.extension_match,
            self.max_depth,
            self.follow_symlinks,
        );

        let search_config = match config {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Error creating search config: {e}");
                std::process::exit(1);
            }
        };

        let lambda: FilterType<S> = |rconfig, rdir, rfilter| {
            {
                rfilter.is_none_or(|f| f(rdir))
                    && rconfig.matches_extension(&rdir.file_name())
                    && rconfig.matches_path(rdir, rconfig.file_name_only)
            }
        };

        Finder {
            root: self.root,
            search_config,
            filter: self.filter,
            custom_filter: lambda,
        }
    }
}