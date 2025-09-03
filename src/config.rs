use crate::size_filter::SizeFilter;
use crate::traits_and_conversions::BytePath as _;

use crate::{DirEntry, DirEntryError, FileType, Result, custom_types_result::BytesStorage};
use regex::bytes::{Regex, RegexBuilder};
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)] //shutup:
/// This struct holds the configuration for searching directories.
///
///
/// It includes options for regex matching, hiding hidden files, keeping directories,
/// matching file extensions, whether to search file names only, depth of search,
/// and whether to follow symlinks.
pub struct SearchConfig {
    pub(crate) regex_match: Option<Regex>,
    ///a regex to match against the file names
    ///if this is None, then the pattern is empty or just a dot, so we
    ///match everything, otherwise we match against the regex
    pub(crate) hide_hidden: bool,
    ///if true, then we hide hidden files (those starting with a dot)
    pub(crate) keep_dirs: bool,
    ///if true, then we keep directories in the results, otherwise we only return non-directory files
    pub(crate) extension_match: Option<Box<[u8]>>,
    ///if this is Some, then we match against the extension of the file otherwise accept (if none)
    pub(crate) file_name_only: bool,
    ///if true, then we only match against the file name, otherwise we match against the full path when regexing
    pub(crate) depth: Option<u16>,
    ///the maximum depth to search, if None then no limit
    pub(crate) follow_symlinks: bool, //if true, then we follow symlinks, otherwise we do not follow them
    /// a size filter
    pub(crate) size_filter: Option<SizeFilter>,
    /// a type filter
    pub(crate) type_filter: Option<u8>,
}

impl SearchConfig {
    // Constructor for SearchConfig
    // Builds a regex matcher if a valid pattern is provided, otherwise stores None
    // Returns an error if the regex compilation fails
    #[allow(clippy::fn_params_excessive_bools)]
    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        pattern: impl AsRef<str>,
        hide_hidden: bool,
        case_insensitive: bool,
        keep_dirs: bool,
        file_name_only: bool,
        extension_match: Option<Box<[u8]>>,
        depth: Option<u16>,
        follow_symlinks: bool,
        size_filter: Option<SizeFilter>,
        type_filter: Option<u8>,
    ) -> Result<Self> {
        let patt = pattern.as_ref();
        // If pattern is "." or empty, we do not filter by regex, this avoids building a regex (even if its trivial cost)
        let regex_match = if patt == "." || patt.is_empty() {
            None
        } else {
            let reg = RegexBuilder::new(patt)
                .case_insensitive(case_insensitive)
                .dot_matches_new_line(false)
                .build();

            if let Err(regerror) = reg {
                return Err(DirEntryError::RegexError(regerror));
            }
            reg.ok()
        };

        Ok(Self {
            regex_match,
            hide_hidden,
            keep_dirs,
            extension_match,
            file_name_only,
            depth,
            follow_symlinks,
            size_filter,
            type_filter,
        })
    }

    #[inline]
    #[must_use]
    /// Evaluates a custom predicate function against a path
    pub fn matches_with<F: FnOnce(&[u8]) -> bool>(&self, path: &[u8], predicate: F) -> bool {
        predicate(path)
    }

    #[inline]
    /// Applies the size filter to a directory entry if configured
    /// Works differently for regular files vs symlinks (resolves symlinks first)
    pub fn matches_extension<S>(&self, entry: &S) -> bool
    where
        S: core::ops::Deref<Target = [u8]>,
    {
        debug_assert!(
            !entry.contains(&b'/'),
            "the filename contains a slash, some arithmetic has gone wrong somewhere!"
        ); // ensure that the entry is a file name and not a path
        self.extension_match
            .as_ref()
            .is_none_or(|ext| entry.matches_extension(ext))
    }
    #[inline]
    #[must_use]
    #[cfg(target_os = "linux")] //FOR EXPERIMENTAL REASONS, ITS LINUX ONLY FOR NOW (EASE OF TESTING)
    #[allow(clippy::if_not_else)] // this is a stylistic choice to avoid unnecessary else branches
    pub(crate) fn matches_path_internal(
        &self,
        dir: &[u8],
        full_path: bool,
        path_len: usize,
    ) -> bool {
        debug_assert!(path_len <= dir.len(), "path_len is greater than dir length");

        self.regex_match.as_ref().is_none_or(|reg| {
            reg.is_match(if !full_path {
                // SAFETY: path_len is guaranteed to be <= dir.len()
                // so slicing with get_unchecked(path_len..) is always within bounds.
                unsafe { dir.get_unchecked(path_len..) }
            } else {
                dir
            })
        })
    }

    #[inline]
    #[must_use]
    /// Applies the size filter to a directory entry if configured.
    /// Works differently for regular files vs symlinks (resolves symlinks first).
    pub fn matches_size<S>(&self, entry: &DirEntry<S>) -> bool
    where
        S: BytesStorage,
    {
        let Some(filter_size) = self.size_filter else {
            return true; // No size filter configured
        };

        #[allow(clippy::wildcard_enum_match_arm)]
        match entry.file_type() {
            FileType::RegularFile => entry
                .size()
                .ok()
                .is_some_and(|file_size| filter_size.is_within_size(file_size as u64)),
            FileType::Symlink => {
                entry
                    // If symlink, resolve to full path and check if it points to a regular file
                    .to_full_path()
                    .ok()
                    .filter(DirEntry::is_regular_file)
                    .and_then(|_| entry.size().ok())
                    .is_some_and(|file_size| filter_size.is_within_size(file_size as u64))
            }
            _ => false, // Other file types are not size-filtered
        }
    }

    #[inline]
    #[must_use]
    /// Applies a type filter (single-character code for file type)
    /// Supports common file types: file, dir, symlink, device, pipe, etc
    pub fn matches_type<S>(&self, entry: &DirEntry<S>) -> bool
    where
        S: BytesStorage,
    {
        let Some(type_filter) = self.type_filter else {
            return true;
        };

        match type_filter {
            b'f' => entry.is_regular_file(),
            b'd' => entry.is_dir(),
            b'l' => entry.is_symlink(),
            b'p' => entry.is_pipe(),
            b'c' => entry.is_char_device(),
            b'b' => entry.is_block_device(),
            b's' => entry.is_socket(),
            b'u' => entry.is_unknown(),
            b'x' => entry.is_executable(),
            b'e' => entry.is_empty(),
            _ => false,
        }
    }

    #[inline]
    #[must_use]
    #[allow(clippy::if_not_else)] // this is a stylistic choice to avoid unnecessary else branches
    /// Checks if the path or file name matches the regex filter
    /// If `full_path` is false, only checks the filename
    pub fn matches_path<S>(&self, dir: &DirEntry<S>, full_path: bool) -> bool
    where
        S: BytesStorage,
    {
        debug_assert!(
            !dir.file_name().contains(&b'/'),
            "file_name contains a directory separator"
        );
        self.regex_match.as_ref().is_none_or(|reg| {
            reg.is_match(if !full_path {
                dir.file_name() //this is the likelier path so we choose it first
            } else {
                dir
            })
        })
    }
}
