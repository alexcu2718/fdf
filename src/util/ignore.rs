#![allow(dead_code)]
// TODO, gonna be a pain to rewrite for Windows as it is...
use std::env::{current_dir, home_dir, var_os};
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::fs::canonicalize;
use std::io::BufReader;
use std::io::Read as _;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub struct IgnoreMatcher {
    start_dir: Option<PathBuf>,
}

static HOME_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(home_dir);
static CURRENT_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(|| current_dir().ok());
static XDG_CONFIG_HOME: LazyLock<Option<OsString>> = LazyLock::new(|| var_os("XDG_CONFIG_HOME"));

static CURRENT_DIR_CANONICAL: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| canonicalize(CURRENT_DIR.as_deref()?).ok());

/// A WIP implementation of an ignore matcher
impl IgnoreMatcher {
    pub fn from_current_dir<A: AsRef<OsStr>>(start_dir: A) -> Self {
        Self {
            start_dir: canonicalize(start_dir.as_ref()).ok(),
        }
    }

    /// Returns ignore entries from git's global excludes file(s), and from
    /// the current directory `.gitignore` only when `start_dir` resolves to
    /// the same canonical path as the current directory.
    pub fn gitconfig_contents(&self) -> Option<Vec<u8>> {
        let mut output = vec![];
        let mut found_any = false;

        for config_path in global_gitconfig_candidates() {
            for excludes_file in parse_excludes_files_from_config(&config_path) {
                if append_filtered_ignore_lines(&excludes_file, &mut output) {
                    found_any = true;
                }
            }
        }

        if self.should_read_current_dir_gitignore()
            && let Some(local_path) = CURRENT_DIR.as_ref().map(|path| path.join(".gitignore"))
            && append_filtered_ignore_lines(&local_path, &mut output)
        {
            found_any = true;
        }

        found_any.then_some(output)
        //TODO! use glob to regex to parse these
        // this may be a little tricky, fix it another time, this is still WIP
    }

    // If the calling process is in a git directory, use the .gitignore found there
    fn should_read_current_dir_gitignore(&self) -> bool {
        match (self.start_dir.as_ref(), CURRENT_DIR_CANONICAL.as_ref()) {
            (Some(start_dir_canonical), Some(current_dir_canonical)) => {
                start_dir_canonical == current_dir_canonical
            }
            _ => false,
        }
    }
}

fn global_gitconfig_candidates() -> Vec<PathBuf> {
    let mut paths = vec![];

    // Always add ~/.gitconfig if home exists
    if let Some(home) = HOME_DIR.as_deref() {
        paths.push(home.join(".gitconfig"));
    }

    // Determine the XDG path based on the tuple of options
    match (XDG_CONFIG_HOME.as_deref(), HOME_DIR.as_deref()) {
        (Some(xdg), _) if !xdg.is_empty() => {
            paths.push(PathBuf::from(xdg).join("git/config"));
        }
        (None, Some(home)) => {
            paths.push(home.join(".config/git/config"));
        }
        // Handles cases where:
        // - XDG is Some but empty (do nothing for XDG path)
        // - XDG is None and home is None (do nothing for fallback)
        _ => {}
    }

    paths
}

fn parse_excludes_files_from_config(config_path: &Path) -> Vec<PathBuf> {
    let mut files = vec![];
    let Some(mut reader) = File::open(config_path).ok().map(BufReader::new) else {
        return files;
    };

    let mut raw = vec![];
    if reader.read_to_end(&mut raw).is_err() {
        return files;
    }

    let mut in_core_section = false;

    for raw_line in raw.split(|byte| *byte == b'\n') {
        let trimmed = trim_ascii_whitespace(strip_trailing_cr(raw_line));

        if let Some(section) = trimmed
            .strip_prefix(b"[")
            .and_then(|value| value.strip_suffix(b"]"))
        {
            in_core_section = section.eq_ignore_ascii_case(b"core");
            continue;
        }

        if !in_core_section {
            continue;
        }

        let (key, value) = match split_once_byte(trimmed, b'=') {
            Some((key, value)) => (trim_ascii_whitespace(key), trim_ascii_whitespace(value)),
            None => continue,
        };

        if !key.eq_ignore_ascii_case(b"excludesFile") {
            continue;
        }

        let value_without_comment_raw =
            split_once_byte(value, b'#').map_or(value, |(head, _)| head);
        let value_without_comment = trim_ascii_whitespace(value_without_comment_raw);
        for token in value_without_comment
            .split(u8::is_ascii_whitespace)
            .filter(|token| !token.is_empty())
        {
            if let Some(path) = expand_config_path(token, config_path) {
                files.push(path);
            }
        }
    }

    files
}

fn expand_config_path(raw: &[u8], config_path: &Path) -> Option<PathBuf> {
    if raw == b"~" {
        return HOME_DIR.as_ref().cloned();
    }

    if let Some(stripped_bytes) = raw.strip_prefix(b"~/") {
        let path_from_home: PathBuf = OsStr::from_bytes(stripped_bytes).into();
        return HOME_DIR.as_ref().map(|home| home.join(path_from_home));
    }

    let path: PathBuf = OsStr::from_bytes(raw).into();
    if path.is_absolute() {
        Some(path)
    } else {
        config_path
            .parent()
            .map(|parent| parent.join(&path))
            .or(Some(path))
    }
}

fn split_once_byte(bytes: &[u8], needle: u8) -> Option<(&[u8], &[u8])> {
    let index = bytes.iter().position(|byte| *byte == needle)?;
    let (left, right) = bytes.split_at(index);
    right.split_first().map(|(_, rest)| (left, rest))
}

fn append_filtered_ignore_lines(path: &Path, output: &mut Vec<u8>) -> bool {
    let Some(mut reader) = File::open(path).ok().map(BufReader::new) else {
        return false;
    };

    let mut raw = vec![];
    if reader.read_to_end(&mut raw).is_err() {
        return false;
    }

    let mut appended = false;
    for raw_line in raw.split(|byte| *byte == b'\n') {
        let trimmed_line = trim_ascii_whitespace(strip_trailing_cr(raw_line));
        if trimmed_line.is_empty() || trimmed_line.starts_with(b"#") {
            continue;
        }

        output.extend_from_slice(trimmed_line);
        output.push(b'\n');
        appended = true;
    }

    appended
}

fn strip_trailing_cr(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\r").unwrap_or(bytes)
}

const fn trim_ascii_whitespace(mut bytes: &[u8]) -> &[u8] {
    while let Some((first, rest)) = bytes.split_first() {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }

    while let Some((last, rest)) = bytes.split_last() {
        if last.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }

    bytes
}
