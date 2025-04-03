#![allow(dead_code)]
use crate::{glob_to_regex, DirEntryError, Result};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DOT_PATTERN: &str = ".";
const START_PREFIX: &str = "/";

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
    args_directory: Option<std::ffi::OsString>,
    canonicalise: bool,
) -> std::ffi::OsString {
    if args_cd {
        std::env::current_dir().map_or_else(
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
        let path_check = std::path::Path::new(&dir_to_use);

        if !path_check.is_dir() {
            eprintln!("{dir_to_use:?} is not a directory");
            std::process::exit(1);
        }

        if canonicalise {
            match path_check.canonicalize() {
                //stupid yank spelling.
                Ok(canonical_path) => canonical_path.into_os_string(),
                Err(e) => {
                    eprintln!("Failed to canonicalise path {path_check:?}: {e}");
                    std::process::exit(1);
                }
            }
        } else {
            dir_to_use
        }
    }
}

/// Get the length of the basename of a path (up to and including the last '/')
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn get_baselen(path: &[u8]) -> u8 {
    path.rsplitn(2, |&c| c == b'/')
        .nth(1)
        .map_or(1, |parent| parent.len() + 1) as u8 // +1 to include trailing slash etc
}

/// Convert Unix timestamp (seconds + nanoseconds) to `SystemTime`
#[allow(clippy::missing_errors_doc)] //fixing errors later
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
pub fn unix_time_to_system_time(sec: i64, nsec: i32) -> Result<SystemTime> {
    let (base, offset) = if sec >= 0 {
        (UNIX_EPOCH, Duration::new(sec as u64, nsec as u32))
    } else {
        let sec_abs = sec.unsigned_abs();
        (
            UNIX_EPOCH + Duration::new(sec_abs, 0),
            Duration::from_nanos(nsec as u64),
        )
    };

    base.checked_sub(offset)
        .or_else(|| UNIX_EPOCH.checked_sub(Duration::from_secs(0)))
        .ok_or(DirEntryError::TimeError)
}

