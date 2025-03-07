use crate::DirEntry;
use fnmatch_regex2::glob_to_regex;
use memchr::memchr_iter;
use regex::escape as RegexEscape;
const DOT_PATTERN: &str = ".";
const START_PREFIX: &str = "/";
use std::env::current_dir;
use std::ffi::OsString;
use std::path::Path;

#[allow(clippy::inline_always)]
#[inline(always)]
#[must_use]
pub fn get_depth(filename: &[u8]) -> usize {
    let count = memchr_iter(b'/', filename).count();

    if filename.is_empty() && filename[0] == b'/' {
        count.saturating_sub(1)
    } else {
        count
    }
}

#[allow(clippy::inline_always)]
#[inline(always)]
#[allow(clippy::missing_errors_doc)]
pub fn read_dir(path: &[u8]) -> Result<Vec<DirEntry>, std::io::Error> {
    DirEntry::new(path)
}

#[must_use]
pub fn process_glob_regex(glob_pattern: &str, args_glob: bool) -> String {
    if !args_glob {
        return glob_pattern.into();
    }

    glob_to_regex(glob_pattern).map_or_else(
        |_| {
            eprintln!("This can't be processed as a glob pattern");
            std::process::exit(1)
        },
        |good_pattern| good_pattern.as_str().into(),
    )
}

#[must_use]
pub fn escape_regex_string(input: &str, avoid_regex: bool, args_glob: bool) -> String {
    if !avoid_regex || args_glob {
        return input.into();
    }
    RegexEscape(input)
}

#[allow(clippy::must_use_candidate)]
pub fn resolve_directory(args_cd: bool, args_directory: Option<OsString>) -> OsString {
    if args_cd
        || args_directory
            .as_ref()
            .is_some_and(|check_dot| check_dot == DOT_PATTERN)
    {
        current_dir().map_or_else(
            |_| DOT_PATTERN.into(),
            |path_res| {
                path_res
                    .to_str()
                    .map_or_else(|| DOT_PATTERN.into(), Into::into)
            },
        )
    } else {
        let dir_to_use = args_directory.unwrap_or_else(|| START_PREFIX.into());
        let path_check = Path::new(&dir_to_use);
        if !path_check.is_dir() {
            eprintln!("{dir_to_use:?} is not a directory");
            std::process::exit(1)
        }
        dir_to_use
    }
}
