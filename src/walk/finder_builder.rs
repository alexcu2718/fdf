use crate::{
    SearchConfigError, config, filters,
    fs::DirEntry,
    walk::{DirEntryFilter, FilterType, finder::Finder},
};
use core::num::NonZeroU32;
use dashmap::DashSet;
use std::{
    ffi::{OsStr, OsString},
    fs::metadata,
    os::unix::fs::MetadataExt as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/**
 A builder for creating a `Finder` instance with customisable options.

 This builder allows you to set various options such as hiding hidden files, case sensitivity,
 keeping directories in results, matching file extensions, setting maximum search depth,
 following symlinks, and applying a custom filter function.
*/
#[expect(
    clippy::struct_excessive_bools,
    reason = "Naturally a builder will contain many bools"
)]
pub struct FinderBuilder {
    pub(crate) root: OsString,
    pub(crate) pattern: Option<String>,
    pub(crate) hide_hidden: bool,
    pub(crate) case_insensitive: bool,
    pub(crate) keep_dirs: bool,
    pub(crate) file_name_only: bool,
    pub(crate) extension_match: Option<Box<[u8]>>,
    pub(crate) max_depth: Option<NonZeroU32>,
    pub(crate) follow_symlinks: bool,
    pub(crate) filter: Option<DirEntryFilter>,
    pub(crate) size_filter: Option<filters::SizeFilter>,
    pub(crate) time_filter: Option<filters::TimeFilter>,
    pub(crate) file_type: Option<filters::FileTypeFilter>,
    pub(crate) collect_errors: bool,
    pub(crate) use_glob: bool,
    pub(crate) canonicalise: bool,
    pub(crate) same_filesystem: bool,
    pub(crate) thread_count: usize,
}

impl FinderBuilder {
    /**
      Creates a new `FinderBuilder` with required fields.

      # Arguments
      `root` - The root directory to search
    */
    pub fn new<A: AsRef<OsStr>>(root: A) -> Self {
        const MIN_THREADS: usize = 1;
        let num_threads =
            std::thread::available_parallelism().map_or(MIN_THREADS, core::num::NonZeroUsize::get);
        Self {
            root: root.as_ref().to_owned(),
            pattern: None,
            hide_hidden: true,
            case_insensitive: true,
            keep_dirs: false,
            file_name_only: true,
            extension_match: None,
            max_depth: None,
            follow_symlinks: false,
            filter: None,
            size_filter: None,
            time_filter: None,
            file_type: None,
            collect_errors: false,
            use_glob: false,
            canonicalise: false,
            same_filesystem: false,
            thread_count: num_threads,
        }
    }

    /// Set the search pattern (regex or glob)
    #[must_use]
    pub fn pattern<P: AsRef<str>>(mut self, pattern: P) -> Self {
        self.pattern = Some(pattern.as_ref().into());
        self
    }

    /// Set whether to hide hidden files, defaults to true
    #[must_use]
    pub const fn keep_hidden(mut self, hide_hidden: bool) -> Self {
        self.hide_hidden = hide_hidden;
        self
    }
    /// Set case insensitive matching,defaults to true
    #[must_use]
    pub const fn case_insensitive(mut self, case_insensitive: bool) -> Self {
        self.case_insensitive = case_insensitive;
        self
    }

    /// Set whether to keep directories in results,defaults to false
    #[must_use]
    pub const fn keep_dirs(mut self, keep_dirs: bool) -> Self {
        self.keep_dirs = keep_dirs;
        self
    }

    /// Set whether to use short paths in regex/glob matching, defaults to true
    /// This is over-ridden if the search term contains a '/'
    #[must_use]
    pub const fn file_name_only(mut self, short_path: bool) -> Self {
        self.file_name_only = short_path;
        self
    }

    /// Set extension to match, defaults to no extension
    #[must_use]
    pub fn extension<C: AsRef<str>>(mut self, extension: C) -> Self {
        let ext = extension.as_ref().as_bytes();

        if ext.is_empty() {
            self.extension_match = None;
        } else {
            self.extension_match = Some(ext.into());
        }

        self
    }

    /// Set maximum search depth
    #[must_use]
    pub const fn max_depth(mut self, max_depth: Option<u32>) -> Self {
        match max_depth {
            None => self,
            Some(num) => {
                self.max_depth = core::num::NonZeroU32::new(num);
                self
            }
        }
    }

    /// Sets size-based filtering criteria.
    #[must_use]
    pub const fn filter_by_size(mut self, size_of: Option<filters::SizeFilter>) -> Self {
        self.size_filter = size_of;
        self
    }

    /// Sets time-based filtering criteria for file modification times.
    #[must_use]
    pub const fn filter_by_time(mut self, time_of: Option<filters::TimeFilter>) -> Self {
        self.time_filter = time_of;
        self
    }

    /// Sets whether to follow symlinks (default: false).
    ///
    /// This will not recurse infinitely but can provide more results than expected
    #[must_use]
    pub const fn follow_symlinks(mut self, follow_symlinks: bool) -> Self {
        self.follow_symlinks = follow_symlinks;
        self
    }

    /// Set a custom filter
    #[must_use]
    pub const fn filter(mut self, filter: Option<fn(&DirEntry) -> bool>) -> Self {
        self.filter = filter;
        self
    }

    /// Sets file type filtering.
    #[must_use]
    pub const fn type_filter(mut self, filter: Option<filters::FileTypeFilter>) -> Self {
        self.file_type = filter;
        self
    }

    /// Sets a glob pattern for regex matching, not a regex.
    #[must_use]
    pub const fn use_glob(mut self, use_glob: bool) -> Self {
        self.use_glob = use_glob;
        self
    }

    /// Set whether to collect errors during traversal, defaults to false
    #[must_use]
    pub const fn collect_errors(mut self, yesorno: bool) -> Self {
        self.collect_errors = yesorno;
        self
    }

    /// Set whether to canonicalise (resolve absolute path) the root directory, defaults to false
    #[must_use]
    pub const fn canonicalise_root(mut self, canonicalise: bool) -> Self {
        self.canonicalise = canonicalise;
        self
    }

    /// Set whether to escape any regexs in the string, defaults to false
    #[must_use]
    pub fn fixed_string(mut self, fixed_string: bool) -> Self {
        if fixed_string {
            self.pattern = self.pattern.as_ref().map(|patt| regex::escape(patt));
        }
        self
    }

    /// Set how many threads rayon is to use, defaults to max
    #[must_use]
    pub const fn thread_count(mut self, threads: Option<usize>) -> Self {
        match threads {
            Some(num) => self.thread_count = num,
            None => return self,
        }

        self
    }

    /// Set whether to follow the same filesystem as root
    #[must_use]
    pub const fn same_filesystem(mut self, yesorno: bool) -> Self {
        self.same_filesystem = yesorno;
        self
    }

    /**
    Builds a [`Finder`] instance with the configured options.

    This method performs validation of all configuration parameters and
    initialises the necessary components for file system traversal.


     # Returns
     Returns `Ok(Finder)` on successful configuration, or
     `Err(SearchConfigError)` if any validation fails.

    # Errors
    Returns an error if:
    - The root path is not a directory or cannot be accessed
    - The root path cannot be canonicalised (when enabled)
    - The search pattern cannot be compiled to a valid regular expression
    - File system metadata cannot be retrieved (for same-filesystem tracking)
    */
    #[allow(clippy::let_underscore_must_use)]
    pub fn build(self) -> core::result::Result<Finder, SearchConfigError> {
        // Resolve and validate the root directory
        let resolved_root = self.resolve_directory()?;

        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(self.thread_count)
            .build_global(); //Skip the error, it only errors if it's already been initialised
        //we do this to avoid passing pools to every iterator (shared access locks etc.)

        let starting_filesystem = if self.same_filesystem {
            // Get the filesystem ID of the root directory directly
            let metadata = metadata(resolved_root.as_ref())?;
            Some(metadata.dev()) // dev() returns the filesystem ID on Unix
        } else {
            None
        };

        let search_config = config::SearchConfig::new(
            self.pattern.as_ref(),
            self.hide_hidden,
            self.case_insensitive,
            self.keep_dirs,
            self.file_name_only,
            self.extension_match,
            self.max_depth,
            self.follow_symlinks,
            self.size_filter,
            self.file_type,
            self.time_filter,
            self.collect_errors,
            self.use_glob,
        )?;

        let lambda: FilterType = |rconfig, rdir, rfilter| {
            {
                rfilter.is_none_or(|func| func(rdir))
                    && rconfig.matches_type(rdir)
                    && rconfig.matches_extension(&rdir.file_name())
                    && rconfig.matches_size(rdir)
                    && rconfig.matches_time(rdir)
                    && rconfig.matches_path(rdir, !rconfig.file_name_only)
            }
        };

        let inode_cache: Option<DashSet<(u64, u64)>> = self.follow_symlinks.then(DashSet::new);

        Ok(Finder {
            root: resolved_root,
            search_config,
            filter: self.filter,
            custom_filter: lambda,
            starting_filesystem,
            inode_cache,
            errors: self
                .collect_errors
                .then(|| Arc::new(Mutex::new(Vec::new()))),
        })
    }

    /**
     Resolves and validates the root directory path.

      This function handles:
      - Default to current directory (".") if root is empty
      - Validates that the path is a directory
      - Optionally canonicalises the path if canonicalise flag is set
    */
    fn resolve_directory(&self) -> core::result::Result<Box<OsStr>, SearchConfigError> {
        let dir_to_use = if self.root.is_empty() {
            // Get current directory and canonicalise it for consistency
            std::env::current_dir().map(PathBuf::into_os_string)?
        } else {
            self.root.clone()
        };

        let path_check = Path::new(&dir_to_use);

        // Validate that the path exists and is a directory
        if !path_check.is_dir() {
            return Err(SearchConfigError::NotADirectory);
        }

        // Apply canonicalisation if requested
        match (self.canonicalise, path_check.canonicalize()) {
            (true, Ok(good_path)) => Ok(good_path.as_os_str().into()),
            _ => Ok(dir_to_use.into_boxed_os_str()),
        }
    }
}
