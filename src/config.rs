use regex::bytes::{Regex, RegexBuilder};

#[derive(Clone)]
pub struct SearchConfig {
    pub regex_match: Option<Regex>,
    pub hide_hidden: bool,
    pub keep_dirs: bool,
    pub extension_match: Option<Box<[u8]>>,
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
    ) -> Self {
        let reg = if pattern == "." {
            None
        } else {
            let actual_pattern = if file_name {
                format!(r".*?(?:^|.*/)(.*{pattern}.*?)$")
            } else {
                pattern.to_string()
            };

            let reg = RegexBuilder::new(&actual_pattern)
                .case_insensitive(case_insensitive)
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
        }
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn matches_with<F>(&self, path: &[u8], predicate: F) -> bool
    where
        F: FnOnce(&[u8]) -> bool,
    {
        predicate(path)
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    #[allow(clippy::unnecessary_map_or)]
    pub fn matches_path(&self, path: &[u8]) -> bool {
        self.regex_match
            .as_ref()
            .map_or(true, |reg| reg.is_match(path))
    }
}
