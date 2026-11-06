#![allow(clippy::missing_inline_in_public_items)]
use crate::fs::DirEntry;
use core::fmt;
use libc::{
    EACCES, EAGAIN, EBADF, EBUSY, EEXIST, EFAULT, EFBIG, EINTR, EINVAL, EIO, EISDIR, ELOOP, EMFILE,
    ENAMETOOLONG, ENFILE, ENOENT, ENOMEM, ENOTDIR, EOVERFLOW, EPERM, ETXTBSY,
};
use std::io;

/// Represents a traversal error encountered during directory processing
#[derive(Debug, Clone)]
pub struct TraversalError {
    /// The directory entry that caused the error
    pub(crate) dir: DirEntry,
    /// The error that occurred
    pub(crate) error: DirEntryError,
}

impl TraversalError {
    /// Get the directory path that caused the error
    #[must_use]
    pub const fn path(&self) -> &DirEntry {
        &self.dir
    }

    /// Get the underlying error
    #[must_use]
    pub const fn error(&self) -> &DirEntryError {
        &self.error
    }
}

impl fmt::Display for TraversalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error accessing {}: {}", self.dir, self.error)
    }
}

/**
This enum encapsulates all possible I/O errors that can occur during filesystem operations,

Comprehensive filesystem I/O error type mapping `io::ErrorKind` to meaningful variants.
*/
#[derive(Debug, Clone, Copy)]
#[allow(clippy::exhaustive_enums)]
pub enum FilesystemIOError {
    /**
    Permission denied for file system access
    Maps to: `io::ErrorKind::PermissionDenied`
    OS codes: EACCES, EPERM
    */
    AccessDenied(io::ErrorKind),
    /**
    Operation temporarily blocked, should be retried
    Maps to: `io::ErrorKind::Interrupted`, `io::ErrorKind::WouldBlock`
    OS codes: EAGAIN, EINTR
    */
    TemporarilyUnavailable,
    /**
    Invalid path or null pointer
    Maps to: `io::ErrorKind::InvalidInput`
    OS codes: EINVAL, EFAULT
    */
    InvalidPath,
    /**
    I/O error occurred while reading from filesystem
    Maps to: `io::ErrorKind::Other` (with `raw_os_error` == EIO)
    OS code: EIO
    */
    FilesystemIO(io::ErrorKind),
    /**
    Symbolic link recursion limit exceeded
    Maps to: Raw OS code ELOOP (no direct `ErrorKind` equivalent)
    OS code: ELOOP
    */
    TooManySymbolicLinks,
    /**
    Pathname component exceeds system limits
    Maps to: Raw OS code ENAMETOOLONG (no direct `ErrorKind` equivalent)
    OS code: ENAMETOOLONG
    */
    NameTooLong,
    /**
    The named file does not exist
    Maps to: `io::ErrorKind::NotFound`
    OS code: ENOENT
    */
    FileNotFound,
    /**
    Out of memory for filesystem operations
    Maps to: `io::ErrorKind::OutOfMemory`
    OS code: ENOMEM
    */
    OutOfMemory,
    /**
    Path exists but is not a directory
    Maps to: `io::ErrorKind::NotADirectory`
    OS code: ENOTDIR
    */
    NotADirectory,
    /**
    Broken pipe error during output operations
    Maps to: `io::ErrorKind::BrokenPipe`
    */
    BrokenPipe(io::ErrorKind),
    /**
    File already exists
    Maps to: `io::ErrorKind::AlreadyExists`
    OS code: EEXIST
    */
    FileExists(io::ErrorKind),
    /**
    Path refers to a directory but operation requires file
    Maps to: `io::ErrorKind::IsADirectory`
    OS code: EISDIR
    */
    IsDirectory(io::ErrorKind),
    /**
    File too large
    Maps to: Raw OS codes EFBIG, EOVERFLOW (no direct `ErrorKind` equivalent)
    OS codes: EFBIG, EOVERFLOW
    */
    FileTooLarge(io::ErrorKind),
    /**
    Resource busy or locked
    Maps to: `io::ErrorKind::ResourceBusy`
    OS codes: EBUSY, ETXTBSY
    */
    ResourceBusy(io::ErrorKind),
    /**
    Invalid file descriptor
    Maps to: `io::ErrorKind::NotSeekable`
    OS code: EBADF
    */
    InvalidFileDescriptor(io::ErrorKind),
    /**
    Process file descriptor limit reached
    Maps to: Raw OS code EMFILE (no direct `ErrorKind` equivalent)
    OS code: EMFILE
    */
    ProcessFileLimitReached(io::ErrorKind),
    /**
    System-wide file descriptor limit reached
    Maps to: Raw OS code ENFILE (no direct `ErrorKind` equivalent)
    OS code: ENFILE
    */
    SystemFileLimitReached(io::ErrorKind),
    /**
    Unsupported operation
    Maps to: `io::ErrorKind::Unsupported`
    */
    UnsupportedOperation(io::ErrorKind),
    /**
    Unhandled OS error
    Any other error that doesn't map to specific variants
    */
    Other(io::ErrorKind),
}

impl fmt::Display for FilesystemIOError {
    #[allow(clippy::pattern_type_mismatch, clippy::use_debug)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccessDenied(kind) => write!(f, "Permission denied error ({kind:?})"),
            Self::TemporarilyUnavailable => {
                write!(f, "Operation temporarily unavailable, retry later")
            }
            Self::InvalidPath => write!(f, "Invalid path or null pointer"),
            Self::FilesystemIO(kind) => write!(f, "Filesystem I/O error ({kind:?})"),
            Self::TooManySymbolicLinks => {
                write!(f, "Too many symbolic links (recursion limit exceeded)")
            }
            Self::NameTooLong => write!(f, "Pathname component exceeds system limits"),
            Self::FileNotFound => write!(f, "File or directory not found"),
            Self::OutOfMemory => write!(f, "Out of memory for filesystem operation"),
            Self::NotADirectory => write!(f, "Path exists but is not a directory"),
            Self::BrokenPipe(kind) => write!(f, "Broken pipe ({kind:?})"),
            Self::FileExists(kind) => write!(f, "File already exists ({kind:?})"),
            Self::IsDirectory(kind) => write!(f, "Path refers to a directory ({kind:?})"),
            Self::FileTooLarge(kind) => write!(f, "File too large ({kind:?})"),
            Self::ResourceBusy(kind) => write!(f, "Resource busy or locked ({kind:?})"),
            Self::InvalidFileDescriptor(kind) => write!(f, "Invalid file descriptor ({kind:?})"),
            Self::ProcessFileLimitReached(kind) => {
                write!(f, "Process file descriptor limit reached ({kind:?})")
            }
            Self::SystemFileLimitReached(kind) => {
                write!(f, "System-wide file descriptor limit reached ({kind:?})")
            }
            Self::UnsupportedOperation(kind) => write!(f, "Unsupported operation ({kind:?})"),
            Self::Other(kind) => write!(f, "OS error ({kind:?})"),
        }
    }
}

impl<E> From<E> for FilesystemIOError
where
    E: Into<std::io::Error>,
{
    fn from(error: E) -> Self {
        let io_error = error.into();
        Self::from_io_error(&io_error)
    }
}

impl FilesystemIOError {
    /// Create a new `FilesystemIOError` from a `std::io::Error`
    #[must_use]
    pub fn from_io_error(error: &io::Error) -> Self {
        let kind = error.kind();
        // Try to map OS error codes first (higher priority)
        if let Some(code) = error.raw_os_error() {
            return match code {
                // Permission and access errors
                EACCES | EPERM => Self::AccessDenied(kind),

                // Temporary/retryable errors
                EAGAIN | EINTR => Self::TemporarilyUnavailable,

                // Path and file existence errors
                EINVAL | EFAULT => Self::InvalidPath,
                ENOENT => Self::FileNotFound,
                EEXIST => Self::FileExists(kind),
                EISDIR => Self::IsDirectory(kind),
                ENOTDIR => Self::NotADirectory,

                // Symbolic link and path resolution errors
                ELOOP => Self::TooManySymbolicLinks,
                ENAMETOOLONG => Self::NameTooLong,

                // I/O and device errors
                EIO => Self::FilesystemIO(kind),
                EBADF => Self::InvalidFileDescriptor(kind),

                // Resource and quota errors
                ENOMEM => Self::OutOfMemory,
                EFBIG | EOVERFLOW => Self::FileTooLarge(kind),

                // File state and locking errors
                EBUSY | ETXTBSY => Self::ResourceBusy(kind),

                // Resource limit errors
                EMFILE => Self::ProcessFileLimitReached(kind),
                ENFILE => Self::SystemFileLimitReached(kind),

                _ => Self::Other(kind),
            };
        }

        // Fall back to mapping std error kinds
        match kind {
            io::ErrorKind::BrokenPipe => Self::BrokenPipe(kind),
            io::ErrorKind::NotFound => Self::FileNotFound,
            io::ErrorKind::PermissionDenied => Self::AccessDenied(kind),
            io::ErrorKind::NotADirectory => Self::NotADirectory,
            io::ErrorKind::AlreadyExists => Self::FileExists(kind),
            io::ErrorKind::IsADirectory => Self::IsDirectory(kind),
            io::ErrorKind::NotSeekable => Self::InvalidFileDescriptor(kind),
            io::ErrorKind::ResourceBusy => Self::ResourceBusy(kind),
            io::ErrorKind::Unsupported => Self::UnsupportedOperation(kind),
            io::ErrorKind::UnexpectedEof => Self::FilesystemIO(kind),
            io::ErrorKind::OutOfMemory => Self::OutOfMemory,
            _ => Self::Other(kind),
        }
    }
}

/**
Errors that can occur when reading or processing directory entries

This enum encapsulates various failure modes when working with filesystem
directory listings, including timestamp conversion, path encoding, and I/O issues.
*/
#[derive(Debug, Clone, Copy)]
#[allow(clippy::exhaustive_enums)]
pub enum DirEntryError {
    /**
    Time conversion or timestamp processing failed

    Occurs when file timestamps cannot be converted to valid datetime values,
    such as with out-of-range values or unsupported timestamp formats.
    */
    TimeError,
    /**
    Path contains invalid UTF-8 sequences

    The directory entry's filename cannot be converted to a UTF-8 string.
    Contains the underlying [`Utf8Error`](core::str::Utf8Error) for details.
    */
    Utf8Error(core::str::Utf8Error),
    /**
    Invalid null bytes detected in filename

    The filename contains interior null bytes, making it invalid for C string conversion.
    Stores the byte position where the null byte was found.
    */
    NulError(usize),
    /**
    Filesystem I/O error during directory operations

    Represents low-level filesystem errors like permission denied, device errors,
    or directory structure corruption. Contains detailed error context.
    */
    IOError(FilesystemIOError),
}

impl From<io::Error> for DirEntryError {
    fn from(error: io::Error) -> Self {
        Self::IOError(FilesystemIOError::from_io_error(&error))
    }
}

impl From<FilesystemIOError> for DirEntryError {
    fn from(error: FilesystemIOError) -> Self {
        Self::IOError(error)
    }
}

impl From<core::str::Utf8Error> for DirEntryError {
    fn from(e: core::str::Utf8Error) -> Self {
        Self::Utf8Error(e)
    }
}

impl From<std::ffi::NulError> for DirEntryError {
    fn from(e: std::ffi::NulError) -> Self {
        Self::NulError(e.nul_position())
    }
}

impl fmt::Display for DirEntryError {
    #[allow(clippy::pattern_type_mismatch)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TimeError => write!(f, "Invalid time conversion"),
            Self::Utf8Error(e) => write!(f, "UTF-8 conversion error: {e}"),
            Self::NulError(pos) => write!(f, "Invalid null byte detected at position {pos}"),
            Self::IOError(e) => write!(f, "I/O error: {e}"),
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
    GlobToRegexError(crate::util::Error),
    /// Invalid regular expression syntax
    RegexError(regex::Error),
    /// I/O error during search configuration or execution
    IOError(io::Error),
    /// Error during directory traversal operation
    TraversalError(DirEntryError),
    /// Specified root path is not a directory
    NotADirectory,
}

impl From<io::Error> for SearchConfigError {
    fn from(error: io::Error) -> Self {
        Self::IOError(error)
    }
}

#[allow(clippy::pattern_type_mismatch)]
impl fmt::Display for SearchConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GlobToRegexError(e) => write!(f, "Glob to regex conversion error: {e}"),
            Self::RegexError(e) => write!(f, "Regex error: {e}"),
            Self::IOError(e) => write!(f, "IO error: {e}"),
            Self::NotADirectory => write!(f, "Path is not a directory"),
            Self::TraversalError(e) => write!(f, "Traversal error: {e}"),
        }
    }
}

impl core::error::Error for SearchConfigError {}
