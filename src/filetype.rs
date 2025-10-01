use libc::{
    DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG, DT_SOCK, DT_UNKNOWN, S_IFBLK, S_IFCHR,
    S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK, mode_t,
};

use crate::DirEntry;
use std::{os::unix::fs::FileTypeExt as _, path::Path};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[expect(
    clippy::exhaustive_enums,
    reason = "This is exhaustive (there aren't anymore filetypes than this)"
)]
/// Represents the type of a file in the filesystem
///
/// This enum provides a cross-platform abstraction for file types with
/// specialised support for Unix filesystem semantics. It can be constructed
/// from various sources including dirent `d_type` values, stat mode bits,
/// and path-based lookups.
///
/// # Examples
/// ```
/// use fdf::FileType;
/// use libc::DT_DIR;
///
/// // Create from dirent d_type
/// let dir_type = FileType::from_dtype(DT_DIR);
/// assert!(dir_type.is_dir());
///
/// ```
///
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
    /// Converts a libc `dirent` `d_type` value to a `FileType`
    ///
    /// This is the preferred method when `d_type` is available, as it avoids
    /// expensive filesystem lookups. However, note that some filesystems
    /// may not support `d_type` or may set it to `DT_UNKNOWN`.
    ///
    /// # Parameters
    /// - `d_type`: The file type from a `dirent` structure
    ///
    /// # Examples
    /// ```
    /// use fdf::FileType;
    /// use libc::{DT_DIR, DT_REG};
    ///
    /// assert!(FileType::from_dtype(DT_DIR).is_dir());
    /// assert!(FileType::from_dtype(DT_REG).is_regular_file());
    /// ```
    pub const fn from_dtype(d_type: u8) -> Self {
        match d_type {
            DT_REG => Self::RegularFile,
            DT_DIR => Self::Directory,
            DT_BLK => Self::BlockDevice,
            DT_CHR => Self::CharDevice,
            DT_FIFO => Self::Pipe,
            DT_LNK => Self::Symlink,
            DT_SOCK => Self::Socket,
            DT_UNKNOWN => Self::Unknown,
            // SAFETY: d_type can only be one of the above
            // Provide a hint to the optimer to realise it can only take these values
            _ => unsafe { core::hint::unreachable_unchecked() },
        }
    }

    #[must_use]
    #[inline]
    #[expect(clippy::if_not_else, reason = "We pick the likelier branch first")]
    /// Fallback method to determine file type when `d_type` is unavailable or `DT_UNKNOWN`
    ///
    /// This method first checks the `d_type` value, and if it's `DT_UNKNOWN`,
    /// falls back to a more expensive lstat-based lookup using the file path.
    ///
    /// # Parameters
    /// - `d_type`: The file type from a dirent structure
    /// - `file_path`: The path cstr to use
    ///
    /// # Notes
    /// While ext4 and BTRFS (and others, not entirely tested!) typically provide reliable `d_type` values,
    /// other filesystems like NTFS, XFS, or FUSE-based filesystems
    /// may require the fallback path.
    pub fn from_dtype_fallback(d_type: u8, file_path: &std::ffi::CStr) -> Self {
        if d_type != libc::DT_UNKNOWN {
            Self::from_dtype(d_type)
        } else {
            DirEntry::get_lstat_private(file_path.as_ptr())
                .map(|statted| Self::from_stat(&statted))
                .unwrap_or(Self::Unknown)
        }
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
    /// Converts Unix mode bits to a `FileType`
    ///
    /// This extracts the file type from the `st_mode` field of a stat structure.
    ///
    /// # Parameters
    /// - `mode`: The mode bits from a stat structure
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
    /// Determines file type using the standard library's metadata lookup
    ///
    /// This method is primarily useful for verification and testing purposes,
    /// not for use within performance-critical iteration code paths.
    ///
    /// # Parameters
    /// - `path_start`: The path to examine
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
    /// Converts a `libc::stat` structure to a `FileType`
    ///
    /// Useful when you already have a stat structure and want to avoid
    /// additional filesystem lookups.
    ///
    /// # Parameters
    /// - `stat`: The stat structure to extract the file type from
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
