//this probably needs to be reworked to reduce struct size, im not knowledgeable enough yet to do so.
use libc::{EACCES, EINVAL, ELOOP, ENOENT, ENOTDIR};
use std::{fmt, io};

#[derive(Debug)]
pub enum DirEntryError {
    InvalidPath,
    InvalidStat,
    TimeError,
    MetadataError,
    Utf8Error(std::str::Utf8Error),
    BrokenPipe(io::Error),
    OSerror(io::Error),
    AccessDenied(io::Error),
    WriteError(io::Error),
    RayonError(rayon::ThreadPoolBuildError),
    RegexError(regex::Error),
    NotADirectory,

    TooManySymbolicLinks, //this shouldnt happen because im ignoring symlinks but this makes it easier to debug
}

impl From<io::Error> for DirEntryError {
    fn from(error: io::Error) -> Self {
        // handle specific error kinds first
        if error.kind() == io::ErrorKind::BrokenPipe {
            return Self::BrokenPipe(error);
        }

        // map OS error codes to variants
        if let Some(code) = error.raw_os_error() {
            match code {
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

impl From<std::str::Utf8Error> for DirEntryError {
    fn from(e: std::str::Utf8Error) -> Self {
        Self::Utf8Error(e)
    }
}

impl fmt::Display for DirEntryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath => write!(f, "Invalid path, neither a file nor a directory"),
            Self::InvalidStat => write!(f, "Invalid file stat"),
            Self::TimeError => write!(f, "Invalid time conversion"),
            Self::MetadataError => write!(f, "Metadata error"),
            Self::Utf8Error(e) => write!(f, "UTF-8 conversion error: {e}"),
            Self::BrokenPipe(e) => write!(f, "Broken pipe: {e}"),
            Self::OSerror(e) => write!(f, "OS error: {e}"),
            Self::RayonError(e) => write!(f, "Rayon error: {e}"),
            Self::WriteError(e) => write!(f, "Write error: {e}"),
            Self::AccessDenied(e) => write!(f, "Access denied: {e}"),
            Self::RegexError(e) => write!(f, "Regex error: {e}"),
            Self::NotADirectory => write!(f, "Not a directory"),
            Self::TooManySymbolicLinks => write!(f, "Too many symbolic links"),
        }
    }
}

impl std::error::Error for DirEntryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Utf8Error(e) => Some(e),
            Self::BrokenPipe(e) | Self::OSerror(e) => Some(e),
            _ => None,
        }
    }
}
