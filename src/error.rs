use core::fmt;
use libc::{EACCES, EAGAIN, EINVAL, ELOOP, ENOENT, ENOTDIR};
use std::io;

#[derive(Debug)]
#[allow(clippy::exhaustive_enums)]
/// Comprehensive error type for directory entry operations and file system traversal.
///
/// This enum encapsulates all possible errors that can occur during directory
/// operations, including I/O errors, permission issues, path validation failures,
/// and system-specific error conditions. It provides detailed error context
/// for robust error handling in file system utilities.
pub enum DirEntryError {
    /// The specified path does not exist or is invalid
    InvalidPath,
    /// File stat information is corrupted or unavailable
    InvalidStat,
    /// Time conversion or timestamp processing failed
    TimeError,
    /// File metadata could not be retrieved or parsed
    MetadataError,
    /// Operation temporarily blocked (e.g., EAGAIN/EWOULDBLOCK)
    TemporarilyUnavailable,
    /// Path contains invalid UTF-8 sequences
    Utf8Error(core::str::Utf8Error),
    /// Broken pipe error during output operations
    BrokenPipe(io::Error),
    /// General operating system error
    OSerror(io::Error),
    /// Permission denied for file system access
    AccessDenied(io::Error),
    /// File write operation failed
    WriteError(io::Error),
    /// Path exists but is not a directory
    NotADirectory,
    /// Symbolic link recursion limit exceeded
    TooManySymbolicLinks,

    NullError,
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
            Self::NullError => write!(f, "Invalid nulls detected in name! "),
            Self::TemporarilyUnavailable => {
                write!(f, "Operation temporarily unavailable, retry later")
            }
            Self::MetadataError => write!(f, "Metadata error"),
            Self::Utf8Error(e) => write!(f, "UTF-8 conversion error: {e}"),
            Self::BrokenPipe(e) => write!(f, "Broken pipe: {e}"),
            Self::OSerror(e) => write!(f, "OS error: {e}"),
            Self::WriteError(e) => write!(f, "Write error: {e}"),
            Self::AccessDenied(e) => write!(f, "Access denied: {e}"),
            Self::NotADirectory => write!(f, "Not a directory"),
            Self::TooManySymbolicLinks => write!(f, "Too many symbolic links"),
        }
    }
}

/// Error type for search configuration and pattern compilation failures.
///
/// This enum handles errors specific to search configuration, including
/// pattern compilation, regex errors, and search initialisation failures.
/// It wraps lower-level errors for unified error handling in search operations.
#[derive(Debug)]
#[allow(clippy::exhaustive_enums)]
pub enum SearchConfigError {
    /// Failed to convert glob pattern to regular expression
    GlobToRegexError(crate::glob::Error),
    /// Invalid regular expression syntax
    RegexError(regex::Error),
    /// I/O error during search configuration or execution
    IoError(io::Error),
    /// Specified root path is not a directory
    NotADirectory,
    /// Error during directory traversal operation
    TraversalError(DirEntryError),
}
impl From<io::Error> for SearchConfigError {
    fn from(error: io::Error) -> Self {
        Self::IoError(error)
    }
}
#[allow(clippy::pattern_type_mismatch)]
impl fmt::Display for SearchConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GlobToRegexError(e) => write!(f, "Glob to regex conversion error: {e}"),
            Self::RegexError(e) => write!(f, "Regex error: {e}"),
            Self::IoError(e) => write!(f, "IO error: {e}"),
            Self::NotADirectory => write!(f, "Path is not a directory"),
            Self::TraversalError(e) => write!(f, "Traversal error: {e}"),
        }
    }
}

impl core::error::Error for SearchConfigError {}
