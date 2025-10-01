#![allow(dead_code)] //some traits are not used yet in full implementation but are used in tests/for future CLI/lib use

//need to add these todo
use crate::{
    DirEntry, DirEntryError, FileType, PathBuffer, Result, access_dirent,
    memchr_derivations::memrchr,
};

use core::cell::Cell;
use core::{fmt, ops::Deref};
#[cfg(not(target_os = "linux"))]
use libc::dirent as dirent64;
#[cfg(target_os = "linux")]
use libc::dirent64;

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
/// A trait for types that dereference to a byte slice (`[u8]`) representing file paths.
/// Provides efficient path operations, FFI compatibility, and filesystem interactions.
/// Explicitly non public to prevent abuse/safety reasons
pub trait BytePath<T>
where
    T: Deref<Target = [u8]> + ?Sized,
{
    fn extension(&self) -> Option<&[u8]>;
    /// Checks if file extension matches given bytes (case-insensitive)
    fn matches_extension(&self, ext: &[u8]) -> bool;

    /// Gets file metadata via `lstat`
    /// Gets file metadata via `stat`
    /// Converts to `&Path` (zero-cost on Unix)
    fn as_path(&self) -> &Path;
    /// Gets index of filename component start
    ///
    /// Returns position after last '/' or 0 if none.
    fn file_name_index(&self) -> u16;
    /// Converts to `&OsStr` (zero-cost)
    fn as_os_str(&self) -> &OsStr;

    /// Creates directory entry from self
    fn to_direntry(&self) -> Result<DirEntry>;

    /// Splits path into components (split on '/')
    fn components(&self) -> impl Iterator<Item = &[u8]>;

    /// Converts to UTF-8 string (with validation)
    fn as_str(&self) -> Result<&str>;
    /// Converts to UTF-8 string without validation
    ///
    /// # Safety
    /// Requires valid UTF-8 bytes (true on Unix)
    unsafe fn as_str_unchecked(&self) -> &str;
    /// Converts to string with invalid UTF-8 replaced
    fn to_string_lossy(&self) -> std::borrow::Cow<'_, str>;
}

impl<T> BytePath<T> for T
where
    T: Deref<Target = [u8]>,
{
    #[inline]
    fn extension(&self) -> Option<&[u8]> {
        // SAFETY: self.len() is guaranteed to be at least 1, as we don't expect empty filepaths (avoid UB check)
        memrchr(b'.', unsafe { self.get_unchecked(..self.len() - 1) }) //exclude cases where the . is the final character
            // SAFETY: The `pos` comes from `memrchr` which searches a slice of `self`.
            // The slice `..self.len() - 1` is a subslice of `self`.
            // Therefore, `pos` is a valid index into `self`.
            // `pos + 1` is also guaranteed to be a valid index.
            // We do this to avoid any runtime checks
            .map(|pos| unsafe { self.get_unchecked(pos + 1..) })
    }

    #[inline]
    fn to_direntry(&self) -> Result<DirEntry> {
        // Convert the byte slice to an OsStr and then to a DirEntry

        DirEntry::new(self.as_os_str())
    }

    #[inline]
    fn matches_extension(&self, ext: &[u8]) -> bool {
        self.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    #[inline]
    fn as_path(&self) -> &Path {
        //&[u8] <=> &OsStr <=> &Path on linux
        self.as_os_str().as_ref()
    }

    #[inline]
    ///cheap conversion from byte slice to `OsStr`
    fn as_os_str(&self) -> &OsStr {
        std::os::unix::ffi::OsStrExt::from_bytes(self)
        //we do it this way because it avoids using the trait from std::fs (avoid needing to import it)
    }

    #[inline]
    // Returns an iterator over the components of the path.
    /// This splits the path by '/' and filters out empty components.
    fn components(&self) -> impl Iterator<Item = &[u8]> {
        self.split(|&b| b == b'/').filter(|s| !s.is_empty())
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    /// Returns the path as a `Result<&str>`
    fn as_str(&self) -> Result<&str> {
        core::str::from_utf8(self).map_err(crate::DirEntryError::Utf8Error)
    }

    #[inline]
    /// Returns the path as a &str without checking if it is valid UTF-8.
    /// # Safety
    /// The caller must ensure that the bytes in `self.path` form valid UTF-8.
    #[allow(clippy::missing_panics_doc)]
    unsafe fn as_str_unchecked(&self) -> &str {
        // SAFETY: The caller must ensure the path is valid utf8 before hand
        unsafe { core::str::from_utf8_unchecked(self) }
    }

    #[inline]
    ///Returns the path as a    `Cow<str>`
    fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(self)
    }

    /// Get the length of the basename of a path (up to and including the last '/')
    #[inline]
    fn file_name_index(&self) -> u16 {
        memrchr(b'/', self).map_or(1, |pos| (pos + 1) as _)
    }
}

impl fmt::Display for DirEntry {
    //i might need to change this to show other metadata.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_lossy())
    }
}

impl From<DirEntry> for PathBuf {
    #[inline]
    fn from(entry: DirEntry) -> Self {
        entry.as_os_str().into()
    }
}
impl TryFrom<&[u8]> for DirEntry {
    type Error = DirEntryError;
    #[inline]
    fn try_from(path: &[u8]) -> Result<Self> {
        Self::new(OsStr::from_bytes(path))
    }
}

impl TryFrom<&OsStr> for DirEntry {
    type Error = DirEntryError;
    #[inline]
    fn try_from(path: &OsStr) -> Result<Self> {
        Self::new(path)
    }
}

impl AsRef<Path> for DirEntry {
    #[inline]
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl core::fmt::Debug for DirEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Dirent")
            .field("path", &self.to_string_lossy())
            .field("file_name", &self.file_name().to_string_lossy())
            .field("depth", &self.depth)
            .field("file_type", &self.file_type)
            .field("file_name_index", &self.file_name_index)
            .field("inode", &self.inode)
            .field("traversible_cache", &self.is_traversible_cache)
            .finish()
    }
}

///A constructor for making accessing the buffer, filename indexes, depths of the parent path while inside the iterator.
/// More documentation TBD
pub trait DirentConstructor {
    fn path_buffer(&mut self) -> &mut PathBuffer;
    fn file_index(&self) -> usize; //modify name a bit so we dont get collisions.
    fn parent_depth(&self) -> u16;

    #[inline]
    #[allow(unused_unsafe)] //lazy fix for illumos/solaris (where we dont actually dereference the pointer, just return unknown TODO-MAKE MORE ELEGANT)
    unsafe fn construct_entry(&mut self, drnt: *const dirent64) -> DirEntry {
        let base_len = self.file_index();
        // SAFETY: The `drnt` must not be null(checked before hand)
        let full_path = unsafe { crate::utils::construct_path(self.path_buffer(), base_len, drnt) };
        // SAFETY: as above ^^

        let dtype = unsafe { access_dirent!(drnt, d_type) }; //need to optimise this for illumos/solaris TODO!
        // SAFETY: Same as above^
        let inode = unsafe { access_dirent!(drnt, d_ino) };

        DirEntry {
            path: full_path.into(),
            file_type: FileType::from_dtype_fallback(dtype, full_path),
            inode,
            depth: self.parent_depth() + 1,
            file_name_index: base_len as _,
            is_traversible_cache: Cell::new(None), //don't set unless we know we need to
        }
    }
}

impl DirentConstructor for crate::ReadDir {
    #[inline]
    fn path_buffer(&mut self) -> &mut PathBuffer {
        &mut self.path_buffer
    }

    #[inline]
    fn file_index(&self) -> usize {
        self.file_name_index as _
    }

    #[inline]
    fn parent_depth(&self) -> u16 {
        self.parent_depth
    }
}

#[cfg(target_os = "linux")]
impl DirentConstructor for crate::iter::GetDents {
    #[inline]
    fn path_buffer(&mut self) -> &mut crate::PathBuffer {
        &mut self.path_buffer
    }

    #[inline]
    fn file_index(&self) -> usize {
        self.file_name_index as _
    }

    #[inline]
    fn parent_depth(&self) -> u16 {
        self.parent_depth
    }
}
