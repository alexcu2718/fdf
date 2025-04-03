use crate::{DirEntry, DirEntryError, Result};
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
        })
    }

    #[inline(always)]
    #[must_use]
    pub fn matches_with<F: FnOnce(&[u8]) -> bool>(&self, path: &[u8], predicate: F) -> bool {
        predicate(path)
    }

    #[inline(always)]
    #[must_use]
    #[allow(clippy::unnecessary_map_or)]
    pub fn matches_path(&self, dir: &DirEntry, full_path: bool) -> bool {
        let path = if full_path {
            dir.as_bytes()
        } else {
            dir.file_name()
        };

        self.regex_match
            .as_ref()
            .map_or(true, |reg| reg.is_match(path))
    }
}
