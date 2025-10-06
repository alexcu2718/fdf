use core::fmt;
use libc::{
    EACCES, EAGAIN, EBADF, EBUSY, EEXIST, EFAULT, EFBIG, EINTR, EINVAL, EIO, EISDIR, ELOOP, EMFILE,
    ENAMETOOLONG, ENFILE, ENOENT, ENOMEM, ENOTDIR, EOVERFLOW, EPERM, ETXTBSY,
};
use std::io;

#[derive(Debug)]
#[allow(clippy::exhaustive_enums)]
///
/// This enum encapsulates all possible I/O errors that can occur during filesystem
/// operations, with precise mapping from libc error codes to semantic error variants.
/// Comprehensive filesystem I/O error type mapping libc error codes to meaningful variants.
pub enum FilesystemIOError {
    /// Permission denied for file system access (EACCES, EPERM)
    AccessDenied(io::Error),
    /// Operation temporarily blocked (EAGAIN, EINTR)
    TemporarilyUnavailable,
    /// Invalid path or null pointer (EINVAL, EFAULT)
    InvalidPath,
    /// I/O error occurred while reading from filesystem (EIO)
    FilesystemIO(io::Error),
    /// Symbolic link recursion limit exceeded (ELOOP)
    TooManySymbolicLinks,
    /// Pathname component exceeds system limits (ENAMETOOLONG)
    NameTooLong,
    /// The named file does not exist (ENOENT)
    FileNotFound,
    /// Out of memory for filesystem operations (ENOMEM)
    OutOfMemory,
    /// Path exists but is not a directory (ENOTDIR)
    NotADirectory,
    /// Broken pipe error during output operations
    BrokenPipe(io::Error),
    /// File already exists (EEXIST)
    FileExists(io::Error),
    /// Path refers to a directory but operation requires file (EISDIR)
    IsDirectory(io::Error),
    /// File too large (EFBIG, EOVERFLOW)
    FileTooLarge(io::Error),
    /// Resource busy or locked (EBUSY, ETXTBSY)
    ResourceBusy(io::Error),
    /// Invalid file descriptor (EBADF)
    InvalidFileDescriptor(io::Error),
    /// Process file descriptor limit reached (EMFILE)
    ProcessFileLimitReached(io::Error),
    /// System-wide file descriptor limit reached (ENFILE)
    SystemFileLimitReached(io::Error),
    /// Unsupported operation
    UnsupportedOperation(io::Error),
    /// Unhandled OS error
    Other(io::Error),
}

impl fmt::Display for FilesystemIOError {
    #[allow(clippy::pattern_type_mismatch)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccessDenied(e) => write!(f, "Permission denied: {e}"),
            Self::TemporarilyUnavailable => {
                write!(f, "Operation temporarily unavailable, retry later")
            }
            Self::InvalidPath => write!(f, "Invalid path or null pointer"),
            Self::FilesystemIO(e) => write!(f, "Filesystem I/O error: {e}"),
            Self::TooManySymbolicLinks => {
                write!(f, "Too many symbolic links (recursion limit exceeded)")
            }
            Self::NameTooLong => write!(f, "Pathname component exceeds system limits"),
            Self::FileNotFound => write!(f, "File or directory not found"),
            Self::OutOfMemory => write!(f, "Out of memory for filesystem operation"),
            Self::NotADirectory => write!(f, "Path exists but is not a directory"),
            Self::BrokenPipe(e) => write!(f, "Broken pipe: {e}"),
            Self::FileExists(e) => write!(f, "File already exists: {e}"),
            Self::IsDirectory(e) => write!(f, "Path refers to a directory: {e}"),
            Self::FileTooLarge(e) => write!(f, "File too large: {e}"),
            Self::ResourceBusy(e) => write!(f, "Resource busy or locked: {e}"),
            Self::InvalidFileDescriptor(e) => write!(f, "Invalid file descriptor: {e}"),
            Self::ProcessFileLimitReached(e) => {
                write!(f, "Process file descriptor limit reached: {e}")
            }
            Self::SystemFileLimitReached(e) => {
                write!(f, "System-wide file descriptor limit reached: {e}")
            }
            Self::UnsupportedOperation(e) => write!(f, "Unsupported operation: {e}"),
            Self::Other(e) => write!(f, "OS error: {e}"),
        }
    }
}

impl<E> From<E> for FilesystemIOError
where
    E: Into<std::io::Error>,
{
    fn from(error: E) -> Self {
        Self::from_io_error(error.into())
    }
}

impl FilesystemIOError {
    #[allow(clippy::wildcard_enum_match_arm)] //not doing them all...
    /// Create a new `FilesystemIOError` from a `std::io::Error`
    pub fn from_io_error(error: io::Error) -> Self {
        // Map OS error codes to variants based on libc documentation
        if let Some(code) = error.raw_os_error() {
            match code {
                // Permission and access errors
                EACCES | EPERM => Self::AccessDenied(error),

                // Temporary/retryable errors
                EAGAIN | EINTR => Self::TemporarilyUnavailable,

                // Path and file existence errors
                EINVAL | EFAULT => Self::InvalidPath,
                ENOENT => Self::FileNotFound,
                EEXIST => Self::FileExists(error),
                EISDIR => Self::IsDirectory(error),
                ENOTDIR => Self::NotADirectory,

                // Symbolic link and path resolution errors
                ELOOP => Self::TooManySymbolicLinks,
                ENAMETOOLONG => Self::NameTooLong,

                // I/O and device errors
                EIO => Self::FilesystemIO(error),
                EBADF => Self::InvalidFileDescriptor(error),

                // Resource and quota errors
                ENOMEM => Self::OutOfMemory,
                EFBIG | EOVERFLOW => Self::FileTooLarge(error),

                // File state and locking errors
                EBUSY | ETXTBSY => Self::ResourceBusy(error),

                // Resource limit errors
                EMFILE => Self::ProcessFileLimitReached(error),
                ENFILE => Self::SystemFileLimitReached(error),

                _ => Self::Other(error),
            }
        } else {
            // Map std error kinds to our variants
            match error.kind() {
                io::ErrorKind::BrokenPipe => Self::BrokenPipe(error),
                io::ErrorKind::NotFound => Self::FileNotFound,
                io::ErrorKind::PermissionDenied => Self::AccessDenied(error),
                io::ErrorKind::NotADirectory => Self::NotADirectory,
                io::ErrorKind::AlreadyExists => Self::FileExists(error),
                io::ErrorKind::IsADirectory => Self::IsDirectory(error),
                io::ErrorKind::NotSeekable => Self::InvalidFileDescriptor(error),
                io::ErrorKind::ResourceBusy => Self::ResourceBusy(error),
                io::ErrorKind::InvalidFilename => Self::InvalidPath,
                io::ErrorKind::Unsupported => Self::UnsupportedOperation(error),
                io::ErrorKind::UnexpectedEof => Self::FilesystemIO(error),
                io::ErrorKind::OutOfMemory => Self::OutOfMemory,
                _ => Self::Other(error),
            }
        }
    }
}

#[derive(Debug)]
#[allow(clippy::exhaustive_enums)]
pub enum DirEntryError {
    /// Time conversion or timestamp processing failed
    TimeError,
    /// Path contains invalid UTF-8 sequences
    Utf8Error(core::str::Utf8Error),
    /// Invalid nulls detected in filename
    NulError(std::ffi::NulError),
    /// Filesystem I/O error
    IOError(FilesystemIOError),
}

impl From<io::Error> for DirEntryError {
    fn from(error: io::Error) -> Self {
        Self::IOError(FilesystemIOError::from_io_error(error))
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

impl fmt::Display for DirEntryError {
    #[allow(clippy::pattern_type_mismatch)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TimeError => write!(f, "Invalid time conversion"),
            Self::Utf8Error(e) => write!(f, "UTF-8 conversion error: {e}"),
            Self::NulError(e) => write!(f, "Invalid nulls detected in name {e}"),
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
    GlobToRegexError(crate::glob::Error),
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
