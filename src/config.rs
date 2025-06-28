use crate::{DirEntry, DirEntryError, Result, custom_types_result::BytesStorage};
use regex::bytes::{Regex, RegexBuilder};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct SearchConfig {
    pub regex_match: Option<Regex>,
    pub hide_hidden: bool,
    pub keep_dirs: bool,
    pub extension_match: Option<Arc<[u8]>>,
    pub file_name: bool,
    pub depth: Option<u8>,
    pub follow_symlinks: bool,
}

impl SearchConfig {
    #[allow(clippy::fn_params_excessive_bools)]
    #[allow(clippy::missing_errors_doc)]
    pub fn new(
        pattern: impl AsRef<str>,
        hide_hidden: bool,
        case_insensitive: bool,
        keep_dirs: bool,
        file_name: bool,
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
            file_name,
            depth,
            follow_symlinks
        })
    }

    #[inline]
    #[must_use]
    pub fn matches_with<F: FnOnce(&[u8]) -> bool>(&self, path: &[u8], predicate: F) -> bool {
        predicate(path)
    }

    #[inline]
    #[must_use]
    pub fn matches_path<S>(&self, dir: &DirEntry<S>, full_path: bool) -> bool
    where
        S: BytesStorage,

    {

        match &self.regex_match {
        Some(reg) => reg.is_match(
            if full_path {
                dir
            } else {
                dir.file_name()
            },
        ),
        None => true,
        }
   
}
}
