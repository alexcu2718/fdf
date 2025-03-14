use crate::glob_to_regex;
//use fnmatch_regex2::glob_to_regex;

const DOT_PATTERN: &str = ".";
const START_PREFIX: &str = "/";
use std::env::current_dir;
use std::ffi::OsString;
use std::path::Path;

#[must_use]
pub fn process_glob_regex(pattern: &str, args_glob: bool) -> String {
    if !args_glob {
        return pattern.into();
    }

    glob_to_regex(pattern).map_or_else(
        |_| {
            eprintln!("This can't be processed as a glob pattern");
            std::process::exit(1)
        },
        |good_pattern| good_pattern.as_str().into(),
    )
}



#[allow(clippy::must_use_candidate)]
pub fn resolve_directory(
    args_cd: bool,
    args_directory: Option<OsString>,
    canonicalise: bool,
) -> OsString {
    if args_cd {
        current_dir().map_or_else(
            |_| DOT_PATTERN.into(),
            |path_res| {
                let path = if canonicalise {
                    path_res.canonicalize().unwrap_or(path_res)
                } else {
                    path_res
                };
                path.to_str().map_or_else(|| DOT_PATTERN.into(), Into::into)
            },
        )
    } else {
        let dir_to_use = args_directory.unwrap_or_else(|| START_PREFIX.into());
        let path_check = Path::new(&dir_to_use);

        if !path_check.is_dir() {
            eprintln!("{dir_to_use:?} is not a directory");
            std::process::exit(1);
        }

        if canonicalise {
            match path_check.canonicalize() {
                Ok(canonical_path) => canonical_path.into_os_string(),
                Err(e) => {
                    eprintln!("Failed to canonicalize path {path_check:?}: {e}");
                    std::process::exit(1);
                }
            }
        } else {
            dir_to_use
        }
    }
}
