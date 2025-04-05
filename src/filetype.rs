#![allow(clippy::inline_always)]
use crate::ToStat;
use libc::{
    mode_t, DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG, DT_SOCK, S_IFBLK, S_IFCHR, S_IFDIR,
    S_IFIFO, S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK,
};

use std::{ffi::OsStr, os::unix::ffi::OsStrExt, os::unix::fs::FileTypeExt, path::Path};
/// Represents the type of a file in the filesystem
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
    #[inline(always)]
    /// Converts a `libc` file type to a `FileType`
    /// I would *prefer* to use this function instead of the below one.
    /// However on some ESOTERIC/EDGY AHH linux filesystems, this can fuck up
    pub const fn from_dtype(d_type: u8) -> Self {
        match d_type {
            DT_DIR => Self::Directory,
            DT_REG => Self::RegularFile,
            DT_BLK => Self::BlockDevice,
            DT_CHR => Self::CharDevice,
            DT_FIFO => Self::Fifo,
            DT_LNK => Self::Symlink,
            DT_SOCK => Self::Socket,
            _ => Self::Unknown,
        }
    }

    #[must_use]
    #[inline(always)]
    ///this is a fallback for when we can't get the file type from the libc
    ///this can happen on funky filesystems like NTFS
    //im probably going to handle this
    pub fn from_dtype_fallback(d_type: u8, file_path: &[u8]) -> Self {
        match d_type {
            DT_DIR => Self::Directory,
            DT_REG => Self::RegularFile,
            DT_BLK => Self::BlockDevice,
            DT_CHR => Self::CharDevice,
            DT_FIFO => Self::Fifo,
            DT_LNK => Self::Symlink,
            DT_SOCK => Self::Socket,
            _ => Self::from_bytes(file_path),
        }
    }

    #[must_use]
    #[inline(always)]
    ///uses a stat call to get the file type, more costly but more accurate
    /// this is used when we can't get the file type from dirent64 due to funky filesystems
    pub fn from_bytes(file_path: &[u8]) -> Self {
        file_path
            .get_stat()
            .map_or(Self::Unknown, |metadata| Self::from_mode(metadata.st_mode))
    }

    #[must_use]
    #[inline(always)]
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
    /// converts a `FileType` from a path
    #[must_use]
    #[inline(always)]
    pub fn from_path<'a>(path_start: impl AsRef<&'a Path>) -> Self {
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
}

impl std::fmt::Display for FileType {
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
//so lazy but it works lol.
impl<T> From<T> for FileType
where
    T: AsRef<OsStr>,
{
    fn from(os_str: T) -> Self {
        Self::from_bytes(os_str.as_ref().as_bytes())
    }
}
