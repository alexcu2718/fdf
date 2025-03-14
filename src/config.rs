#![allow(clippy::inline_always)]
use crate::DirEntry;
use regex::bytes::{Regex, RegexBuilder};

#[derive(Clone, Debug)]
pub struct SearchConfig {
    pub regex_match: Option<Regex>,
    pub hide_hidden: bool,
    pub keep_dirs: bool,
    pub extension_match: Option<Box<[u8]>>,
    pub file_name: bool,
    pub depth: Option<u16>,
}

impl SearchConfig {
    #[allow(clippy::fn_params_excessive_bools)]
    #[must_use]
    pub fn new(
        pattern: &str,
        hide_hidden: bool,
        case_insensitive: bool,
        keep_dirs: bool,
        file_name: bool,
        extension_match: Option<Box<[u8]>>,
        depth: Option<u16>,
    ) -> Self {
        let reg = if pattern == "." || pattern.is_empty() {
            None
        } else {
            let reg = RegexBuilder::new(pattern)
                .case_insensitive(case_insensitive)
                .dot_matches_new_line(false)
                .build();

            if reg.is_err() {
                eprintln!("Error in regex: {}", reg.unwrap_err());
                std::process::exit(1);
            }
            reg.ok()
        };

        Self {
            regex_match: reg,
            hide_hidden,
            keep_dirs,
            extension_match,
            file_name,
            depth,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn matches_with<F>(&self, path: &[u8], predicate: F) -> bool
    where
        F: FnOnce(&[u8]) -> bool,
    {
        predicate(path)
    }

    #[inline(always)]
    #[must_use]
    #[allow(clippy::unnecessary_map_or)]
    pub fn matches_path(&self, dir: &DirEntry, full_path: bool) -> bool {
        let path = if full_path {
            &dir.path
        } else {
            dir.file_name()
        };

        self.regex_match
            .as_ref()
            .map_or(true, |reg| reg.is_match(path))
    }
}



