use crate::traits_and_conversions::BytePath;
use crate::{DirEntry, DirEntryError, Result, custom_types_result::BytesStorage};
use regex::bytes::{Regex, RegexBuilder};
use std::sync::Arc;

#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)] //shutup
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
    pub(crate) extension_match: Option<Arc<[u8]>>,
    ///if this is Some, then we match against the extension of the file otherwise accept (if none)
    pub(crate) file_name_only: bool,
    ///if true, then we only match against the file name, otherwise we match against the full path when regexing
    pub(crate) depth: Option<u8>,
    ///the maximum depth to search, if None then no limit
    pub(crate) follow_symlinks: bool, //if true, then we follow symlinks, otherwise we do not follow them
}

impl SearchConfig {
    #[allow(clippy::fn_params_excessive_bools)]
    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        pattern: impl AsRef<str>,
        hide_hidden: bool,
        case_insensitive: bool,
        keep_dirs: bool,
        file_name_only: bool,
        extension_match: Option<Arc<[u8]>>,
        depth: Option<u8>,
        follow_symlinks: bool,
    ) -> Result<Self> {
        let patt = pattern.as_ref();

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
        })
    }

    #[inline]
    #[must_use]
    pub fn matches_with<F: FnOnce(&[u8]) -> bool>(&self, path: &[u8], predicate: F) -> bool {
        predicate(path)
    }

    #[inline]
    pub fn matches_extension<S>(&self, entry: &S) -> bool
    where
        S: std::ops::Deref<Target = [u8]>,
    {
        debug_assert!(!entry.contains(&b'/')); // ensure that the entry is a file name and not a path
        self.extension_match
            .as_ref()
            .is_none_or(|ext| entry.matches_extension(ext))
    }
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    #[allow(clippy::if_not_else)] // this is a stylistic choice to avoid unnecessary else branches
    pub(crate) fn matches_path_internal(
        &self,
        dir: &[u8],
        full_path: bool,
        path_len: usize,
    ) -> bool {
        self.regex_match.as_ref().is_none_or(|reg| {
            reg.is_match(if !full_path {
                debug_assert!(path_len <= dir.len(), "path_len is greater than dir length");
                debug_assert!(
                    !(&dir[path_len..]).contains(&b'/'),
                    "filename should not contain a directory separator"
                );
                unsafe { dir.get_unchecked(path_len..) } //this is the likelier path so we choose it first
            } else {
                dir
            })
        })
    }

    #[inline]
    #[must_use]
    #[allow(clippy::if_not_else)] // this is a stylistic choice to avoid unnecessary else branches
    pub fn matches_path<S>(&self, dir: &DirEntry<S>, full_path: bool) -> bool
    where
        S: BytesStorage,
    {
        self.regex_match.as_ref().is_none_or(|reg| {
            reg.is_match(if !full_path {
                debug_assert!(
                    !dir.file_name().contains(&b'/'),
                    "file_name contains a directory separator"
                );

                dir.file_name() //this is the likelier path so we choose it first
            } else {
                dir
            })
        })
    }
}
