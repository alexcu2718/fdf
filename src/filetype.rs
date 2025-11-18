use libc::{
    AT_SYMLINK_FOLLOW, AT_SYMLINK_NOFOLLOW, DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG,
    DT_SOCK, S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK, fstatat,
    mode_t,
};

use crate::FileDes;
use core::ffi::CStr;

use std::{os::unix::fs::FileTypeExt as _, path::Path};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[expect(
    clippy::exhaustive_enums,
    reason = "This is exhaustive (there aren't anymore filetypes than this)"
)]
/**
Represents the type of a file in the filesystem

This enum provides a cross-platform abstraction for file types with
specialised support for Unix filesystem semantics. It can be constructed
from various sources including dirent `d_type` values, stat mode bits,
and path-based lookups.

# Examples

 ```
use fdf::FileType;
use libc::DT_DIR;

// Create from dirent d_type
let dir_type = FileType::from_dtype(DT_DIR);
assert!(dir_type.is_dir());

```
*/
pub enum FileType {
    /// Block special device file (e.g., /dev/sda)
    BlockDevice,
    /// Character special device file (e.g., /dev/tty)
    CharDevice,
    /// Directory
    Directory,
    /// FIFO (named pipe)
    Pipe,
    /// Symbolic link
    Symlink,
    /// Regular file
    RegularFile,
    /// Socket file
    Socket,
    /// Unknown file type (should be rare on supported filesystems)
    Unknown,
}

impl FileType {
    #[must_use]
    #[inline]
    /**
    Converts a libc `dirent` `d_type` value to a `FileType`

    This is the preferred method when `d_type` is available, as it avoids
    expensive filesystem lookups. However, note that some filesystems
    may not support `d_type` or may set it to `DT_UNKNOWN`.

    # Parameters
    - `d_type`: The file type from a `dirent` structure

    # Examples

    ```
    use fdf::FileType;
    use libc::{DT_DIR, DT_REG};

    assert!(FileType::from_dtype(DT_DIR).is_dir());
    assert!(FileType::from_dtype(DT_REG).is_regular_file());
    ```
        */
    pub const fn from_dtype(d_type: u8) -> Self {
        match d_type {
            DT_REG => Self::RegularFile,
            DT_DIR => Self::Directory,
            DT_BLK => Self::BlockDevice,
            DT_CHR => Self::CharDevice,
            DT_FIFO => Self::Pipe,
            DT_LNK => Self::Symlink,
            DT_SOCK => Self::Socket,
            _ /*DT_UNKNOWN */=> Self::Unknown,
        }
    }

    /**
    Determines the file type from a file descriptor and filename without following symlinks

    This method uses `fstatat` with the `AT_SYMLINK_NOFOLLOW` flag to get file
    information from a file descriptor, which is more efficient than
    path-based lookups when you already have an open file descriptor.

    This function will not follow symbolic links - it will return information about
    the link itself rather than the target file.

    # Parameters
    - `fd`: The directory file descriptor (or `AT_FDCWD` for current directory)
    - `filename`: The filename to stat relative to the directory fd

    # Returns
    - `FileType`: The detected file type, or `FileType::Unknown` if the file doesn't exist
      or an error occurred

        */
    #[inline]
    #[must_use]
    pub fn from_fd_no_follow(fd: &FileDes, filename: &CStr) -> Self {
        stat_syscall!(fstatat, fd.0, filename.as_ptr(), AT_SYMLINK_NOFOLLOW, DTYPE)
    }

    #[inline]
    #[must_use]
    /**
    Determines the file type from a file descriptor and filename, following symlinks

    This method uses `fstatat` with the `AT_SYMLINK_FOLLOW` flag to get file
    information from a file descriptor. Unlike `from_fd_no_follow`, this function
    will follow symbolic links and return information about the target file rather
    than the link itself.

    This is equivalent to the standard `stat` system call behavior and is useful
    when you want to know the type of the actual file being accessed, regardless
    of whether it's reached through a symbolic link.

    # Parameters
    - `fd`: The directory file descriptor (or `AT_FDCWD` for current directory)
    - `filename`: The filename to stat relative to the directory fd

    # Returns
    - `FileType`: The detected file type, or `FileType::Unknown` if the file doesn't exist
      or an error occurred

    */
    pub fn from_fd_follow(fd: &FileDes, filename: &CStr) -> Self {
        stat_syscall!(fstatat, fd.0, filename.as_ptr(), AT_SYMLINK_FOLLOW, DTYPE)
    }
    /// Returns true if this represents a directory  (cost free check)
    #[inline]
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        matches!(*self, Self::Directory)
    }

    /// Returns true if this represents a regular file  (cost free check)
    #[inline]
    #[must_use]
    pub const fn is_regular_file(&self) -> bool {
        matches!(*self, Self::RegularFile)
    }

    /// Returns true if this represents a symbolic link  (cost free check)
    #[inline]
    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        matches!(*self, Self::Symlink)
    }

    /// Returns true if this represents a block device  (cost free check)
    #[inline]
    #[must_use]
    pub const fn is_block_device(&self) -> bool {
        matches!(*self, Self::BlockDevice)
    }

    /// Returns true if this represents a character device  (cost free check)
    #[inline]
    #[must_use]
    pub const fn is_char_device(&self) -> bool {
        matches!(*self, Self::CharDevice)
    }

    /// Returns true if this represents a FIFO (named pipe)  (cost free check)
    #[inline]
    #[must_use]
    pub const fn is_pipe(&self) -> bool {
        matches!(*self, Self::Pipe)
    }

    /// Returns true if this represents a socket (cost free check)
    #[inline]
    #[must_use]
    pub const fn is_socket(&self) -> bool {
        matches!(*self, Self::Socket)
    }

    /// Returns true if this represents an unknown file type  (cost free check)
    #[inline]
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(*self, Self::Unknown)
    }

    #[must_use]
    #[inline]
    /**
    Converts a Unix `mode_t` (from `stat.st_mode`) to a `FileType`

     This is used internally by `from_fd` and can also be used directly
     when you already have a stat structure.

     # Parameters
     - `mode`: The `st_mode` field from a stat structure

     # Examples
     ```
     use fdf::FileType;
     use libc::S_IFDIR;

     assert!(FileType::from_mode(S_IFDIR).is_dir());
     ```
    */
    pub const fn from_mode(mode: mode_t) -> Self {
        match mode & S_IFMT {
            S_IFREG => Self::RegularFile,
            S_IFDIR => Self::Directory,
            S_IFBLK => Self::BlockDevice,
            S_IFCHR => Self::CharDevice,
            S_IFIFO => Self::Pipe,
            S_IFLNK => Self::Symlink,
            S_IFSOCK => Self::Socket,
            _ => Self::Unknown,
        }
    }
    /**
    Determines file type using the standard library's metadata lookup

    This method is primarily useful for verification and testing purposes,
    not for use within performance-critical iteration code paths.

    # Parameters
    - `path_start`: The path to examine
    */
    #[must_use]
    #[inline]
    #[allow(clippy::filetype_is_file)] //dumb lint
    pub fn from_path<P: AsRef<Path>>(path_start: P) -> Self {
        Path::new(path_start.as_ref())
            .symlink_metadata()
            .map_or(Self::Unknown, |metadata| match metadata.file_type() {
                ft if ft.is_dir() => Self::Directory,
                ft if ft.is_file() => Self::RegularFile,
                ft if ft.is_symlink() => Self::Symlink,
                ft if ft.is_block_device() => Self::BlockDevice,
                ft if ft.is_char_device() => Self::CharDevice,
                ft if ft.is_fifo() => Self::Pipe,
                ft if ft.is_socket() => Self::Socket,
                _ => Self::Unknown,
            })
    }
    #[must_use]
    #[inline]
    /**
    Converts a `libc::stat` structure to a `FileType`

    Useful when you already have a stat structure and want to avoid
    additional filesystem lookups.

    # Parameters
    - `stat`: The stat structure to extract the file type from
    */
    pub const fn from_stat(stat: &libc::stat) -> Self {
        Self::from_mode(stat.st_mode)
    }
}

impl core::fmt::Display for FileType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::BlockDevice => write!(f, "Block device"),
            Self::CharDevice => write!(f, "Character device"),
            Self::Directory => write!(f, "Directory"),
            Self::Pipe => write!(f, "FIFO"),
            Self::Symlink => write!(f, "Symlink"),
            Self::RegularFile => write!(f, "Regular file"),
            Self::Socket => write!(f, "Socket"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl From<libc::stat> for FileType {
    /// Converts a `libc::stat` directly to a `FileType`
    #[inline]
    fn from(stat: libc::stat) -> Self {
        Self::from_stat(&stat)
    }
}
