#![allow(dead_code)] //some traits are not used yet in full implementation but are used in tests/for future CLI/lib use

use crate::FileDes;
//need to add these todo
use crate::{
    DirEntry, DirEntryError, FileType, Result, access_dirent, memchr_derivations::memrchr,
    utils::dirent_name_length,
};

use core::cell::Cell;
use core::ffi::CStr;
use core::ptr;
use core::{fmt, ops::Deref};
#[cfg(not(target_os = "linux"))]
use libc::dirent as dirent64;
#[cfg(target_os = "linux")]
use libc::dirent64;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

const_from_env!(FDF_MAX_FILENAME_LEN:usize="FDF_MAX_FILENAME_LEN",512); //setting the minimum extra memory it'll need
// this should be ideally 256 but operating systemns can be funky, so i'm being a bit cautious
// as this is written for unix, most except HURD i think allow the filename to be 255 max, I'm not writing this for hurd ffs.
const _: () = assert!(
    //0 cost compile time check that doesnt rely on debug
    FDF_MAX_FILENAME_LEN >= 255,
    "Expect it to always be above this value"
);
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
    fn path_buffer(&mut self) -> &mut Vec<u8>;
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
        let file_type = self.get_filetype_private(dtype, &path);

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
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the length of a path will never be above a u16 (well, i'm just not covering that extreme an edgecase!"
    )]
    fn init_from_direntry(dir_path: &DirEntry) -> (Vec<u8>, u16) {
        let dir_path_in_bytes = dir_path.as_bytes();
        let mut base_len = dir_path_in_bytes.len(); // get length of directory path

        let is_root = dir_path_in_bytes == b"/";

        let needs_slash_u8 = u8::from(!is_root); // check if we need to append a slash
        let needs_slash: usize = usize::from(needs_slash_u8);
        //set a conservative estimate incase it returns something useless
        // Initialise buffer with zeros to avoid uninitialised memory then add the max length of a filename on
        let mut path_buffer = vec![0u8; base_len + needs_slash + FDF_MAX_FILENAME_LEN + 10]; //add 10 for good measure negligible performance cost,
        // Please note future readers, `PATH_MAX` is not the max length of a path, it's simply the maximum length of a path that POSIX functions will take
        // I made this mistake then suffered a segfault to the knee. BEWARB
        let buffer_ptr = path_buffer.as_mut_ptr(); // get the mutable pointer to the buffer
        // SAFETY: the memory regions do not overlap , src and dst are both valid and alignment is trivial (u8)
        unsafe { core::ptr::copy_nonoverlapping(dir_path_in_bytes.as_ptr(), buffer_ptr, base_len) }; // copy path
        #[allow(clippy::multiple_unsafe_ops_per_block)] //dumb
        // SAFETY: write is within buffer bounds
        unsafe {
            *buffer_ptr.add(base_len) = b'/' * needs_slash_u8 // add slash if needed  (this avoids a branch ), either add 0 or  add a slash (multiplication)
        };

        base_len += needs_slash; // update length if slash added

        (path_buffer, base_len as _)
    }

    #[inline]
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

        let path_buffer = self.path_buffer();
        // SAFETY: The `base_len` is guaranteed to be a valid index into `path_buffer`
        let buffer = unsafe { &mut path_buffer.get_unchecked_mut(base_len..) };
        // SAFETY:
        // - `d_name` and `buffer` don't overlap (different memory regions)
        // - Both pointers are properly aligned for byte copying
        // - `name_len` is within `buffer` bounds (checked by debug assertion)
        unsafe { ptr::copy_nonoverlapping(d_name, buffer.as_mut_ptr(), name_len) };

        /*
         SAFETY: the buffer is guaranteed null terminated and we're accessing in bounds
        */
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            CStr::from_bytes_with_nul_unchecked(path_buffer.get_unchecked(..base_len + name_len))
        }
    }

    #[inline]
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    #[allow(clippy::transmute_ptr_to_ptr)]
    #[allow(clippy::wildcard_enum_match_arm)]
    fn get_filetype_private(&self, d_type: u8, path: &CStr) -> FileType {
        match FileType::from_dtype(d_type) {
            FileType::Unknown => {
                // Fall back to fstatat for filesystems that don't provide d_type (DT_UNKNOWN)
                /* SAFETY:
                - `file_index()` points to the start of the file name within `bytes`
                - The slice from this index to the end includes the null terminator
                - The slice is guaranteed to represent a valid C string (thus null terminated) */
                let cstr_name: &CStr = unsafe {
                    CStr::from_bytes_with_nul_unchecked(
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
    fn path_buffer(&mut self) -> &mut Vec<u8> {
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
    fn path_buffer(&mut self) -> &mut Vec<u8> {
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
