#![allow(dead_code)] //some traits are not used yet in full implementation but are used in tests/for future CLI/lib use

use crate::FileDes;
//need to add these todo
use crate::{
    DirEntry, DirEntryError, FileType, LOCAL_PATH_MAX, PathBuffer, Result, access_dirent,
    memchr_derivations::memrchr, utils::dirent_name_length,
};

use core::cell::Cell;
use core::ffi::CStr;
use core::{fmt, ops::Deref};
use core::{mem, ptr};
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
    fn file_name_index(&self) -> usize;
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
    fn file_name_index(&self) -> usize {
        memrchr(b'/', self).map_or(1, |pos| pos + 1)
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

/**
  Internal trait for constructing directory entries during iteration

 This trait provides the necessary components to construct `DirEntry` objects
 from raw `dirent64` structures while maintaining path buffer state, tracking
 file name positions, and managing directory traversal depth.

*/
pub trait DirentConstructor {
    /// Returns a mutable reference to the path buffer used for constructing full paths
    fn path_buffer(&mut self) -> &mut PathBuffer;
    /// Returns the current index in the path buffer where the filename should be appended
    ///
    /// This represents the length of the base directory path before adding the current filename.
    fn file_index(&self) -> usize; //modify name a bit so we dont get collisions.
    /// Returns the depth of the parent directory in the traversal hierarchy
    ///
    /// Depth starts at 0 for the root directory being scanned and increments for each subdirectory.
    fn parent_depth(&self) -> u16;
    /// Returns the file descriptor for the current directory being read
    fn file_descriptor(&self) -> &FileDes;

    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Not expecting a filepath to be >u16::MAX"
    )]
    /// Constructs a `DirEntry` from a raw directory entry pointer
    #[allow(unused_unsafe)] //lazy fix for illumos/solaris (where we dont actually dereference the pointer, just return unknown TODO-MAKE MORE ELEGANT)
    unsafe fn construct_entry(&mut self, drnt: *const dirent64) -> DirEntry {
        let base_len = self.file_index();
        // SAFETY: The `drnt` must not be null (checked before using)
        let dtype = unsafe { access_dirent!(drnt, d_type) }; //need to optimise this for illumos/solaris TODO!
        // SAFETY: Same as above^
        let inode = unsafe { access_dirent!(drnt, d_ino) };

        // SAFETY: The `drnt` must not be null(by precondition)
        let full_path = unsafe { self.construct_path(drnt) };
        let path: Box<CStr> = full_path.into();
        let file_type = self.get_filetype(dtype, &path);

        DirEntry {
            path,
            file_type,
            inode,
            depth: self.parent_depth() + 1,
            file_name_index: base_len as _,
            is_traversible_cache: Cell::new(None), //// Lazy cache for traversal checks
        }
    }
    #[inline]
    #[allow(clippy::transmute_ptr_to_ptr)]
    #[allow(clippy::multiple_unsafe_ops_per_block)] //for the dbug assert
    #[allow(clippy::debug_assert_with_mut_call)] //for debug assert (it's fine)
    #[allow(clippy::undocumented_unsafe_blocks)] //for the debug
    /**
      Constructs a full path by appending the directory entry name to the base path


    */
    unsafe fn construct_path(&mut self, drnt: *const dirent64) -> &CStr {
        let base_len = self.file_index();
        // SAFETY: The `drnt` must not be null (checked before using)
        let d_name = unsafe { access_dirent!(drnt, d_name) };
        // SAFETY: as above
        // Add 1 to include the null terminator
        let name_len = unsafe { dirent_name_length(drnt) + 1 };
        debug_assert!(
            name_len + base_len < LOCAL_PATH_MAX,
            "We don't expect the total length to exceed PATH_MAX!"
        );
        let path_buffer = self.path_buffer();
        // SAFETY: The `base_len` is guaranteed to be a valid index into `path_buffer`
        // by the caller of this function.
        let buffer = unsafe { &mut path_buffer.get_unchecked_mut(base_len..) };
        // SAFETY:
        // - `d_name` and `buffer` don't overlap (different memory regions)
        // - Both pointers are properly aligned for byte copying
        // - `name_len` is within `buffer` bounds (checked by debug assertion)
        unsafe { ptr::copy_nonoverlapping(d_name, buffer.as_mut_ptr(), name_len) };
        debug_assert!(
            unsafe {
                CStr::from_ptr(
                    path_buffer
                        .get_unchecked(..base_len + name_len)
                        .as_ptr()
                        .cast(),
                ) == mem::transmute::<&[u8], &CStr>(
                    path_buffer.get_unchecked(..base_len + name_len),
                )
            },
            "we  expect these to be the same"
        );
        /*
         SAFETY: `d_name` and `buffer` are known not to overlap because `d_name` is
         from a `dirent64` pointer and `buffer` is a slice of `path_buffer`.
         The pointers are properly aligned as they point to bytes. The `name_len`
         is guaranteed to be within the bounds of `buffer` because the total path
         length (`base_len + name_len`) is always less than or equal to `LOCAL_PATH_MAX`,
         which is the capacity of `path_buffer`.
        */
        unsafe { mem::transmute(path_buffer.get_unchecked(..base_len + name_len)) }
    }

    #[inline]
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    #[allow(clippy::transmute_ptr_to_ptr)]
    #[allow(clippy::wildcard_enum_match_arm)]
    fn get_filetype(&self, d_type: u8, path: &CStr) -> FileType {
        match FileType::from_dtype(d_type) {
            FileType::Unknown => {
                // Fall back to fstatat for filesystems that don't provide d_type (DT_UNKNOWN)
                /* SAFETY:
                - `file_index()` points to the start of the file name within `bytes`
                - The slice from this index to the end includes the null terminator
                - The slice is guaranteed to represent a valid C string
                - We transmute the slice into a `&CStr` reference for zero-copy access */
                let cstr_name: &CStr = unsafe {
                    core::mem::transmute(
                        path.to_bytes_with_nul().get_unchecked(self.file_index()..),
                    )
                };
                FileType::from_fd_no_follow(self.file_descriptor(), cstr_name)
            }
            known_type => known_type,
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

    #[inline]
    fn file_descriptor(&self) -> &FileDes {
        self.dirfd()
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
    #[inline]
    fn file_descriptor(&self) -> &FileDes {
        &self.fd
    }
}
