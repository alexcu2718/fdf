#[allow(unused_imports)]
use crate::BytePath;

use libc::{
    DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG, DT_SOCK, S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO,
    S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK, mode_t,
};

use std::{os::unix::fs::FileTypeExt as _, path::Path};
/// Represents the type of a file in the filesystem, tiny enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileType {
    BlockDevice,
    CharDevice,
    Directory,
    Fifo,
    Symlink,
    RegularFile,
    Socket,
    Unknown, //this shouldnt ever happen
}

impl FileType {
    #[must_use]
    #[inline]
    /// Converts a `libc` file type to a `FileType`
    /// I would *prefer* to use this function instead of `from_dtype_fallback` but some some filesystems do not use `d_type` or set to 0
    /// on EXT4/BTRFS this is fine however its not guaranteed so this is not really useful.
    pub const fn from_dtype(d_type: u8) -> Self {
        match d_type {
            DT_REG => Self::RegularFile,
            DT_DIR => Self::Directory,
            DT_BLK => Self::BlockDevice,
            DT_CHR => Self::CharDevice,
            DT_FIFO => Self::Fifo,
            DT_LNK => Self::Symlink,
            DT_SOCK => Self::Socket,
            _ => Self::Unknown,
        }
    }
    #[inline]
    pub const fn is_dir(&self) -> bool {
        matches!(self, Self::Directory)
    }
    #[inline]
    pub const fn is_regular_file(&self) -> bool {
        matches!(self, Self::RegularFile)
    }
    #[inline]
    pub const fn is_symlink(&self) -> bool {
        matches!(self, Self::Symlink)
    }
    #[inline]
    pub const fn is_block_device(&self) -> bool {
        matches!(self, Self::BlockDevice)
    }
    #[inline]
    pub const fn is_char_device(&self) -> bool {
        matches!(self, Self::CharDevice)
    }
    #[inline]
    pub const fn is_fifo(&self) -> bool {
        matches!(self, Self::Fifo)
    }
    #[inline]
    pub const fn is_socket(&self) -> bool {
        matches!(self, Self::Socket)
    }

    #[must_use]
    #[inline]
    ///this is a fallback for when we can't get the file type from libc
    ///this can happen on funky filesystems like NTFS/XFS, BTRFS/ext4 work fine.
    //fortunately we can just check the dtype, if it's unknowm, it means we have to do an lstat call, yay!
    pub fn from_dtype_fallback(d_type: u8, file_path: &[u8]) -> Self {
        //i wouldve just chained the function calls but it's clearer this way
        match d_type {
            DT_REG => Self::RegularFile,
            DT_DIR => Self::Directory,
            DT_BLK => Self::BlockDevice,
            DT_CHR => Self::CharDevice,
            DT_FIFO => Self::Fifo,
            DT_LNK => Self::Symlink,
            DT_SOCK => Self::Socket,
            _ => Self::from_bytes(file_path),
        }
    }

    #[must_use]
    #[inline]
    ///uses a lstat call to get the file type, more costly but more accurate
    /// this is used when we can't get the file type from dirent64 due to funky filesystems
    pub fn from_bytes(file_path: &[u8]) -> Self {
        file_path
            .get_stat()
            .map_or(Self::Unknown, |stat| Self::from_mode(stat.st_mode))
    }

    #[must_use]
    #[inline]
    pub const fn from_mode(mode: mode_t) -> Self {
        match mode & S_IFMT {
            S_IFREG => Self::RegularFile,
            S_IFDIR => Self::Directory,
            S_IFBLK => Self::BlockDevice,
            S_IFCHR => Self::CharDevice,
            S_IFIFO => Self::Fifo,
            S_IFLNK => Self::Symlink,
            S_IFSOCK => Self::Socket,
            _ => Self::Unknown,
        }
    }
    /// converts a `FileType` from a path via stdlib's `Path`  this is handy for verification, its not meant for use within iteration
    #[must_use]
    #[inline]
    pub fn from_path<P: AsRef<Path>>(path_start: P) -> Self {
        Path::new(path_start.as_ref())
            .symlink_metadata()
            .map_or(Self::Unknown, |metadata| match metadata.file_type() {
                ft if ft.is_dir() => Self::Directory,
                ft if ft.is_file() => Self::RegularFile,
                ft if ft.is_symlink() => Self::Symlink,
                ft if ft.is_block_device() => Self::BlockDevice,
                ft if ft.is_char_device() => Self::CharDevice,
                ft if ft.is_fifo() => Self::Fifo,
                ft if ft.is_socket() => Self::Socket,
                _ => Self::Unknown,
            })
    }

    #[must_use]
    #[inline]
    /// Returns the corresponding libc dirent `d_type` value
    pub const fn d_type_value(&self) -> u8 {
        match self {
            Self::RegularFile => DT_REG,
            Self::Directory => DT_DIR,
            Self::BlockDevice => DT_BLK,
            Self::CharDevice => DT_CHR,
            Self::Fifo => DT_FIFO,
            Self::Symlink => DT_LNK,
            Self::Socket => DT_SOCK,
            Self::Unknown => 0, // DT_UNKNOWN
        }
    }
}

impl std::fmt::Display for FileType {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlockDevice => write!(f, "Block device"),
            Self::CharDevice => write!(f, "Character device"),
            Self::Directory => write!(f, "Directory"),
            Self::Fifo => write!(f, "FIFO"),
            Self::Symlink => write!(f, "Symlink"),
            Self::RegularFile => write!(f, "Regular file"),
            Self::Socket => write!(f, "Socket"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}
