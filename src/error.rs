//this probably needs to be reworked to reduce struct size, it's on the todo, not important.
//this is abit rough, but it's not too important. well.
use core::fmt;
use libc::{EACCES, EAGAIN, EINVAL, ELOOP, ENOENT, ENOTDIR};
use std::io;

#[derive(Debug)]
#[allow(clippy::exhaustive_enums)]
/// An error type for directory entry operations.
///
/// This enum represents various errors that can occur when working with directory entries,
/// such as invalid paths, metadata errors, and IO errors.
pub enum DirEntryError {
    InvalidPath,
    InvalidStat,
    TimeError,
    MetadataError,
    TemporarilyUnavailable,
    Utf8Error(core::str::Utf8Error),
    BrokenPipe(io::Error),
    GlobToRegexError(crate::glob::Error),
    OSerror(io::Error),
    AccessDenied(io::Error),
    WriteError(io::Error),
    RegexError(regex::Error),
    NotADirectory,
    TooManySymbolicLinks,
}

impl From<io::Error> for DirEntryError {
    #[allow(clippy::wildcard_enum_match_arm)]
    fn from(error: io::Error) -> Self {
        // handle specific error kinds first
        if error.kind() == io::ErrorKind::BrokenPipe {
            return Self::BrokenPipe(error);
        }

        // map OS error codes to variants
        if let Some(code) = error.raw_os_error() {
            match code {
                EAGAIN => Self::TemporarilyUnavailable, // EAGAIN is not a fatal error, just try again later
                EINVAL | ENOENT => Self::InvalidPath,
                ENOTDIR => Self::NotADirectory,
                ELOOP => Self::TooManySymbolicLinks,
                EACCES => Self::AccessDenied(error),
                _ => Self::OSerror(error),
            }
        } else {
            // handle non-OS errors
            Self::OSerror(error)
        }
    }
}

impl From<core::str::Utf8Error> for DirEntryError {
    fn from(e: core::str::Utf8Error) -> Self {
        Self::Utf8Error(e)
    }
}

impl fmt::Display for DirEntryError {
    #[allow(clippy::pattern_type_mismatch)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath => write!(f, "Invalid path, neither a file nor a directory"),
            Self::InvalidStat => write!(f, "Invalid file stat"),
            Self::TimeError => write!(f, "Invalid time conversion"),
            Self::TemporarilyUnavailable => {
                write!(f, "Operation temporarily unavailable, retry later")
            }
            Self::MetadataError => write!(f, "Metadata error"),
            Self::Utf8Error(e) => write!(f, "UTF-8 conversion error: {e}"),
            Self::BrokenPipe(e) => write!(f, "Broken pipe: {e}"),
            Self::GlobToRegexError(e) => write!(f, "Glob to regex conversion error {e}"),
            Self::OSerror(e) => write!(f, "OS error: {e}"),
            Self::WriteError(e) => write!(f, "Write error: {e}"),
            Self::AccessDenied(e) => write!(f, "Access denied: {e}"),
            Self::RegexError(e) => write!(f, "Regex error: {e}"),
            Self::NotADirectory => write!(f, "Not a directory"),
            Self::TooManySymbolicLinks => write!(f, "Too many symbolic links"),
        }
    }
}
