use crate::SearchConfigError;
use crate::filters::{FileTypeFilter, SizeFilter, TimeFilter};
use crate::fs::{DirEntry, FileType};
use crate::util::BytePath as _;
use crate::util::glob_to_regex;
use core::num::NonZeroU32;
use core::ops::Deref;
use core::time::Duration;
use regex::bytes::{Regex, RegexBuilder};
use std::time::UNIX_EPOCH;
use thread_local::ThreadLocal;

pub struct TLSRegex {
    base: Regex,
    local: ThreadLocal<Regex>,
}

impl Clone for TLSRegex {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            local: ThreadLocal::new(),
        }
    }
}

impl core::fmt::Debug for TLSRegex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TLSRegex")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl TLSRegex {
    const fn new(regex: Regex) -> Self {
        Self {
            base: regex,
            local: ThreadLocal::new(),
        }
    }

    #[inline]
    pub fn is_match(&self, path: &[u8]) -> bool {
        self.local.get_or(|| self.base.clone()).is_match(path)
    }
}

/**
This struct holds the configuration for searching a File system via traversal

It includes options for regex matching, hiding hidden files, keeping directories,
matching file extensions, whether to search file names only, depth of search,
and whether to follow symlinks.
*/
#[derive(Clone, Debug)]
#[expect(clippy::struct_excessive_bools, reason = "It's a CLI tool.")]
pub struct SearchConfig {
    /**
    Regular expression pattern for matching file names or paths

    If `None`, matches all files (equivalent to an empty pattern).
    When `file_name_only` is true, only matches against the base filename.
    Uses thread-local storage for efficient multi-threaded regex matching.
    */
    pub(crate) regex_match: Option<TLSRegex>,

    /**
    Whether to exclude hidden files and directories

    Hidden files are those whose names start with a dot (`.`).
    When true, these files are filtered out from results.
    */
    pub(crate) hide_hidden: bool,

    /**
    File extension to filter by (case-insensitive)

    If `Some`, only files with this extension are matched.
    The extension should not include the leading dot (e.g., `"txt"` not `".txt"`).
    */
    pub(crate) extension_match: Option<Box<[u8]>>,

    /**
    Whether regex matching applies only to filename vs full path

    If true, regular expressions match only against the file's base name.
    If false, regular expressions match against the full path.
    */
    pub(crate) file_name_only: bool,

    /**
    Maximum directory depth to search

    If `Some(n)`, limits traversal to `n` levels deep.
    If `None`, searches directories to unlimited depth.
    */
    pub(crate) depth: Option<NonZeroU32>,

    /**
    Whether to follow symbolic links during traversal

    If true, symbolic links are followed and their targets are processed.
    If false, symbolic links are treated as regular files.
    */
    pub(crate) follow_symlinks: bool,

    /**
    Filter based on file size constraints

    If `Some`, only files matching the size criteria are included.
    Supports minimum, maximum, and exact size matching.
    */
    pub(crate) size_filter: Option<SizeFilter>,

    /**
    Filter based on file type

    If `Some`, only files of the specified type are included.
    Can filter by file, directory, symlink, etc.
    */
    pub(crate) type_filter: Option<FileTypeFilter>,

    /**
    Filter based on file modification time

    If `Some`, only files matching the time criteria are included.
    Supports relative time ranges (e.g., "last 7 days").
    */
    pub(crate) time_filter: Option<TimeFilter>,

    /**
    Whether to respect `.gitignore` files during traversal.

    When true, entries ignored by inherited `.gitignore` rules are skipped.
    */
    pub(crate) respect_gitignore: bool,

    /// Compiled ignore matcher (`--ignore` + `--ignoreg`) backed by thread-local regex clones.
    pub(crate) ignore_match: Option<TLSRegex>,
}
impl SearchConfig {
    /**
    Constructor for `SearchConfig`

    Builds a regex matcher if a valid pattern is provided, otherwise stores None
    Returns an error if the regex compilation fails
    */
    #[expect(
        clippy::fn_params_excessive_bools,
        clippy::too_many_arguments,
        reason = "Internal convenience"
    )]
    pub(crate) fn new<ToStr: AsRef<str>>(
        pattern: Option<&ToStr>, // ultimately this is CLI internal only
        hide_hidden: bool,
        case_insensitive: bool,
        filenameonly: bool,
        extension_match: Option<Box<[u8]>>,
        depth: Option<NonZeroU32>,
        follow_symlinks: bool,
        size_filter: Option<SizeFilter>,
        type_filter: Option<FileTypeFilter>,
        time_filter: Option<TimeFilter>,
        use_glob: bool,
        respect_gitignore: bool,
        ignore_patterns: Vec<String>,
        ignore_glob_patterns: Vec<String>,
    ) -> core::result::Result<Self, SearchConfigError> {
        let (file_name_only, pattern_to_use) = if let Some(patt_ref) = pattern.as_ref() {
            let patt = patt_ref.as_ref();
            let file_name_only = if patt.contains('/') {
                false // Over ride because if it's got a slash, it's gotta be a full path
            } else {
                filenameonly
            };

            let pattern_to_use = if use_glob {
                glob_to_regex(patt).map_err(SearchConfigError::GlobToRegexError)?
            } else {
                patt.into()
            };
            (file_name_only, pattern_to_use)
        } else {
            // No pattern provided, use match-all pattern
            (filenameonly, ".*".into())
        };

        // If pattern is "." or empty, we do not filter by regex, this avoids building a regex (even if its trivial cost)
        let regex_match =
            if pattern_to_use == "." || pattern_to_use == ".*" || pattern_to_use.is_empty() {
                None
            } else {
                let reg = RegexBuilder::new(&pattern_to_use)
                    .case_insensitive(case_insensitive)
                    .dot_matches_new_line(false)
                    .build();

                if let Err(regerror) = reg {
                    return Err(SearchConfigError::RegexError(regerror));
                }
                reg.ok().map(TLSRegex::new)
            };

        let mut ignore_patterns_merged =
            Vec::with_capacity(ignore_patterns.len() + ignore_glob_patterns.len());
        ignore_patterns_merged.extend(ignore_patterns);

        for glob_pattern in ignore_glob_patterns {
            let regex_pattern =
                glob_to_regex(&glob_pattern).map_err(SearchConfigError::GlobToRegexError)?;
            ignore_patterns_merged.push(regex_pattern);
        }

        let ignore_match = if ignore_patterns_merged.is_empty() {
            None
        } else {
            let combined = ignore_patterns_merged
                .iter()
                .map(|patt| format!("(?:{patt})"))
                .collect::<Vec<_>>()
                .join("|");

            let reg = RegexBuilder::new(&combined)
                .case_insensitive(case_insensitive)
                .dot_matches_new_line(false)
                .build()
                .map_err(SearchConfigError::RegexError)?;
            Some(TLSRegex::new(reg))
        };

        Ok(Self {
            regex_match,
            hide_hidden,
            extension_match,
            file_name_only,
            depth,
            follow_symlinks,
            size_filter,
            type_filter,
            time_filter,
            respect_gitignore,
            ignore_match,
        })
    }

    /// Returns true when the provided path should be ignored by configured ignore patterns.
    #[inline]
    #[must_use]
    pub fn matches_ignore_path(&self, path: &[u8]) -> bool {
        self.ignore_match
            .as_ref()
            .is_some_and(|reg| reg.is_match(path))
    }

    /// Evaluates a custom predicate function against a path
    #[inline]
    #[must_use]
    pub fn matches_with<F: Fn(&[u8]) -> bool>(&self, path: &[u8], predicate: F) -> bool {
        predicate(path)
    }

    /// Checks for extension match via memchr
    #[inline]
    pub fn matches_extension<S>(&self, entry: &S) -> bool
    where
        S: Deref<Target = [u8]>,
    {
        debug_assert!(
            !entry.contains(&b'/'),
            "the filename contains a slash, some arithmetic has gone wrong somewhere!"
        ); // Ensure that the entry is a file name and not a path
        self.extension_match
            .as_ref()
            .is_none_or(|ext| entry.matches_extension(ext))
    }

    /**
    Applies the configured size filter to a directory entry, if any.
    For regular files the size is checked directly.
    For symlinks, the target is resolved first and then checked if it is a regular file.
    Other file types are ignored.
    */
    #[inline]
    #[must_use]
    #[allow(clippy::cast_sign_loss)] // Sign loss does not matter here
    pub fn matches_size(&self, entry: &DirEntry) -> bool {
        let Some(filter_size) = self.size_filter else {
            return true; // No filter means always match
        };

        match entry.file_type {
            FileType::RegularFile => entry
                .file_size()
                .ok()
                .is_some_and(|sz| filter_size.is_within_size(sz)),
            //Check if it exists first, then call stat..
            FileType::Symlink => {
                entry.exists()
                    && entry.get_stat().is_ok_and(|statted| {
                        FileType::from_stat(&statted) == FileType::RegularFile
                            && filter_size.is_within_size(statted.st_size as _)
                    })
            }

            _ => false,
        }
    }

    /// Applies a type filter using `FileTypeFilter` enum
    /// Supports common file types: file, dir, symlink, device, pipe, etc
    #[inline]
    #[must_use]
    pub fn matches_type(&self, entry: &DirEntry) -> bool {
        let Some(type_filter) = self.type_filter else {
            return true;
        };

        match type_filter {
            FileTypeFilter::File => entry.is_regular_file(),
            FileTypeFilter::Directory => entry.is_dir(),
            FileTypeFilter::Symlink => entry.is_symlink(),
            FileTypeFilter::Pipe => entry.is_pipe(),
            FileTypeFilter::CharDevice => entry.is_char_device(),
            FileTypeFilter::BlockDevice => entry.is_block_device(),
            FileTypeFilter::Socket => entry.is_socket(),
            FileTypeFilter::Unknown => entry.is_unknown(),
            FileTypeFilter::Executable => entry.is_executable(),
            FileTypeFilter::Empty => entry.is_empty(),
        }
    }

    /// Applies time-based filtering to files based on modification time
    /// Returns true if the file's modification time matches the filter criteria
    #[inline]
    #[must_use]
    pub fn matches_time(&self, entry: &DirEntry) -> bool {
        let Some(time_filter) = self.time_filter else {
            return true; // No filter means always match
        };

        // Get the modification time from the file and convert to SystemTime
        entry
            .modified_time()
            .ok()
            .and_then(|datetime| datetime.timestamp_nanos_opt())
            .and_then(|nanos| UNIX_EPOCH.checked_add(Duration::from_nanos(nanos.cast_unsigned())))
            .is_some_and(|systime| time_filter.matches_time(systime))
    }

    /// Checks if the path or file name matches the regex filter
    /// If `full_path` is false, only checks the filename
    #[inline]
    #[must_use]
    pub fn matches_path(&self, dir: &DirEntry, full_path: bool) -> bool {
        self.regex_match.as_ref().is_none_or(|reg|
                // Use arithmetic to avoid branching costs
             { let index_amount=usize::from(!full_path) * dir.file_name_index();


                     // SAFETY: are always indexing within bounds.
            unsafe{reg.is_match(dir.get_unchecked(index_amount..))}
            })
    }
}
