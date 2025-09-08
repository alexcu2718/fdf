#![allow(dead_code)] //some traits are not used yet in full implementation but are used in tests/for future CLI/lib use
use crate::modified_unix_time_to_datetime;
//need to add these todo
use crate::{
    AlignedBuffer, BytesStorage, DirEntry, DirEntryError, FileType, LOCAL_PATH_MAX, PathBuffer,
    Result, access_dirent, buffer::ValueType, memchr_derivations::memrchr,
};
use chrono::{DateTime, Utc};
use core::{fmt, mem::MaybeUninit, ops::Deref};
#[cfg(not(target_os = "linux"))]
use libc::dirent as dirent64;
#[cfg(target_os = "linux")]
use libc::dirent64;
use libc::{F_OK, R_OK, W_OK, access, lstat, stat};
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
    fn as_cstr_ptr<F, R, VT>(&self, func: F) -> R
    where
        F: FnOnce(*const VT) -> R,
        VT: ValueType; // VT=i8/u8

    fn extension(&self) -> Option<&[u8]>;
    /// Checks if file extension matches given bytes (case-insensitive)
    fn matches_extension(&self, ext: &[u8]) -> bool;
    /// Gets file size in bytes
    fn size(&self) -> Result<i64>;
    /// Gets file metadata via `lstat`
    fn get_lstat(&self) -> Result<stat>;
    /// Gets file metadata via `stat`
    fn get_stat(&self) -> Result<stat>;
    /// Gets last modification time
    fn modified_time(&self) -> Result<DateTime<Utc>>;
    /// Converts to `&Path` (zero-cost on Unix)
    fn as_path(&self) -> &Path;
    /// Opens file descriptor for directory paths
    ///
    /// # Safety
    /// - Path must be a directory
    /// - Uses `O_DIRECTORY | O_CLOEXEC | O_NONBLOCK`
    unsafe fn open_fd(&self) -> Result<i32>;
    /// Gets index of filename component start
    ///
    /// Returns position after last '/' or 0 if none.
    fn file_name_index(&self) -> u16;
    /// Converts to `&OsStr` (zero-cost)
    fn as_os_str(&self) -> &OsStr;
    /// Checks file existence (`access(F_OK)`)
    fn exists(&self) -> bool;
    /// Creates directory entry from self
    fn to_direntry<S>(&self) -> Result<DirEntry<S>>
    where
        S: BytesStorage;
    /// Checks read permission (`access(R_OK)`)
    fn is_readable(&self) -> bool;
    /// Checks write permission (`access(W_OK)`)
    fn is_writable(&self) -> bool;

    /// Splits path into components (split on '/')
    fn components(&self) -> impl Iterator<Item = &[u8]>;
    /// Gets standard filesystem metadata
    fn to_std_file_type(&self) -> Result<std::fs::FileType>;
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
    fn as_cstr_ptr<F, R, VT>(&self, func: F) -> R
    where
        //TODO! change this to unsafe MAYBE?
        F: FnOnce(*const VT) -> R,
        VT: ValueType, // VT==ValueType is u8/i8
    {
        debug_assert!(
            self.len() < LOCAL_PATH_MAX, //declared at compile time via env_var or default to 4096/1024 (Linux/ BSD-like(including macos))
            "Input too large for buffer"
        );
        // TODO! investigate this https://docs.rs/nix/latest/src/nix/lib.rs.html#318-350
        // Essentially I need to check the implications of this.

        let mut c_path_buf_start = AlignedBuffer::<u8, { LOCAL_PATH_MAX }>::new();

        let c_path_buf = c_path_buf_start.as_mut_ptr();

        // copy bytes using copy_nonoverlapping to avoid ub check
        // SAFETY: The source (`self`) and destination (`c_path_buf`) pointers are known not to overlap,
        // and the length is guaranteed to be less than or equal to the destination's capacity (`LOCAL_PATH_MAX`).
        unsafe { core::ptr::copy_nonoverlapping(self.as_ptr(), c_path_buf, self.len()) };
        // SAFETY: The destination is `c_path_buf` offset by `self.len()`, which is a valid index
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            c_path_buf.add(self.len()).write(0)
        }; // Null terminate the string

        func(c_path_buf.cast::<_>())
    }

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
    fn to_direntry<S>(&self) -> Result<DirEntry<S>>
    where
        S: BytesStorage,
    {
        // Convert the byte slice to an OsStr and then to a DirEntry

        DirEntry::<S>::new(self.as_os_str())
    }

    #[inline]
    fn matches_extension(&self, ext: &[u8]) -> bool {
        self.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    #[inline]
    fn size(&self) -> Result<i64> {
        self.get_lstat().map(|s| s.st_size as _)
    }

    #[inline]
    unsafe fn open_fd(&self) -> Result<i32> {
        // Opens the file and returns a file descriptor.
        // This is a low-level operation that may fail if the file does not exist or cannot be opened.
        const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK;
        self.as_cstr_ptr(|ptr| {
            // SAFETY: the pointer is null terminated
            let fd = unsafe { libc::open(ptr, FLAGS) };

            if fd < 0 {
                Err(std::io::Error::last_os_error().into())
            } else {
                Ok(fd)
            }
        })
    }

    #[inline]
    fn get_lstat(&self) -> Result<stat> {
        let mut stat_buf = MaybeUninit::<stat>::uninit();
        // SAFETY: We know the path is valid
        let res = self.as_cstr_ptr(|ptr| unsafe { lstat(ptr, stat_buf.as_mut_ptr()) });

        if res == 0 {
            // SAFETY: If the return code is 0, we know it's been initialised properly
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }

    #[inline]
    fn get_stat(&self) -> Result<stat> {
        let mut stat_buf = MaybeUninit::<stat>::uninit();
        // SAFETY: We know the path is valid
        let res = self.as_cstr_ptr(|ptr| unsafe { stat(ptr, stat_buf.as_mut_ptr()) });

        if res == 0 {
            // SAFETY: If the return code is 0, we know it's been initialised properly
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    fn modified_time(&self) -> Result<DateTime<Utc>> {
        let s = self.get_lstat()?;
        modified_unix_time_to_datetime(&s).ok_or(DirEntryError::TimeError)
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
    fn is_readable(&self) -> bool {
        // SAFETY: The path is guaranteed to be a filepath (when used internally)
        unsafe { self.as_cstr_ptr(|ptr| access(ptr, R_OK)) == 0 }
    }

    #[inline]
    fn is_writable(&self) -> bool {
        //maybe i can automatically exclude certain files from this check to
        //then reduce my syscall total, would need to read into some documentation. zadrot ebaniy
        // SAFETY: The path is guaranteed to be a filepath (when used internally)
        unsafe { self.as_cstr_ptr(|ptr| access(ptr, W_OK)) == 0 }
    }

    #[inline]
    // Returns an iterator over the components of the path.
    /// This splits the path by '/' and filters out empty components.
    fn components(&self) -> impl Iterator<Item = &[u8]> {
        self.split(|&b| b == b'/').filter(|s| !s.is_empty())
    }

    #[inline]
    fn to_std_file_type(&self) -> Result<std::fs::FileType> {
        //  can't directly create a std::fs::FileType,
        // we need to make a system call to get it
        std::fs::symlink_metadata(self.as_path())
            .map(|m| m.file_type())
            .map_err(core::convert::Into::into)
    }

    #[inline]
    ///checks if the file exists, this, makes a syscall
    fn exists(&self) -> bool {
        // SAFETY: The path is guaranteed to be be null terminated
        unsafe { self.as_cstr_ptr(|ptr| access(ptr, F_OK)) == 0 }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::missing_const_for_fn)] //this cant be const clippy be LYING
    /// Returns the path as a `Result<&str>`
    fn as_str(&self) -> Result<&str> {
        core::str::from_utf8(self).map_err(crate::DirEntryError::Utf8Error)
    }

    #[inline]
    #[allow(clippy::missing_const_for_fn)]
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

impl<S> fmt::Display for DirEntry<S>
where
    S: BytesStorage,
{
    //i might need to change this to show other metadata.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_lossy())
    }
}

impl<S> Deref for DirEntry<S>
where
    S: BytesStorage,
{
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.path.as_bytes()
    }
}

impl<S> From<DirEntry<S>> for PathBuf
where
    S: BytesStorage,
{
    #[inline]
    fn from(entry: DirEntry<S>) -> Self {
        entry.as_os_str().into()
    }
}
impl<S> TryFrom<&[u8]> for DirEntry<S>
where
    S: BytesStorage,
{
    type Error = DirEntryError;
    #[inline]
    fn try_from(path: &[u8]) -> Result<Self> {
        Self::new(OsStr::from_bytes(path))
    }
}

impl<S> TryFrom<&OsStr> for DirEntry<S>
where
    S: BytesStorage,
{
    type Error = DirEntryError;
    #[inline]
    fn try_from(path: &OsStr) -> Result<Self> {
        Self::new(path)
    }
}

impl<S> AsRef<Path> for DirEntry<S>
where
    S: BytesStorage,
{
    #[inline]
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl<S> core::fmt::Debug for DirEntry<S>
where
    S: BytesStorage,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Dirent")
            .field("path", &self.to_string_lossy())
            .field("file_name", &self.file_name().to_string_lossy())
            .field("depth", &self.depth)
            .field("file_type", &self.file_type)
            .field("file_name_index", &self.file_name_index)
            .field("inode", &self.inode)
            .finish()
    }
}

///A constructor for making accessing the buffer, filename indexes, depths of the parent path while inside the iterator.
/// More documentation TBD
pub trait DirentConstructor<S: BytesStorage> {
    fn path_buffer(&mut self) -> &mut PathBuffer;
    fn file_index(&self) -> usize; //modify name a bit so we dont get collisions.
    fn parent_depth(&self) -> u16;

    #[inline]
    #[allow(unused_unsafe)] //lazy fix for illumos/solaris (where we dont actually dereference the pointer, just return unknown TODO-MAKE MORE ELEGANT)
    unsafe fn construct_entry(&mut self, drnt: *const dirent64) -> DirEntry<S> {
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
        }
    }
}

impl<S: BytesStorage> DirentConstructor<S> for crate::DirIter<S> {
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
impl<S: BytesStorage> DirentConstructor<S> for crate::iter::DirEntryIterator<S> {
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
