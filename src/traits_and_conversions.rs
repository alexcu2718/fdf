#![allow(clippy::missing_safety_doc)] //adding these later
#![allow(clippy::missing_errors_doc)]
use crate::BytesStorage;
use crate::DirEntry;
use crate::DirEntryError;
use crate::FileType;
use crate::PathBuffer;
use crate::Result;
use crate::buffer::ValueType;
use crate::memchr_derivations::memrchr;
use crate::offset_dirent;
use core::fmt;
use core::mem::MaybeUninit;
use core::mem::transmute;
use core::ops::Deref;
#[cfg(not(target_os = "linux"))]
use libc::dirent as dirent64;
#[cfg(target_os = "linux")]
use libc::dirent64;
use libc::{F_OK, R_OK, W_OK, access, lstat, stat};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

///a trait over anything which derefs to `&[u8]` then convert to *const i8 or *const u8 (inferred ), useful for FFI.
pub trait BytePath<T>
where
    T: Deref<Target = [u8]>,
{
    fn as_cstr_ptr<F, R, VT>(&self, f: F) -> R
    where
        F: FnOnce(*const VT) -> R,
        VT: ValueType; // VT==ValueType is u8/i8

    fn extension(&self) -> Option<&[u8]>;
    fn matches_extension(&self, ext: &[u8]) -> bool;
    fn size(&self) -> crate::Result<u64>;
    fn get_stat(&self) -> crate::Result<stat>;
    fn modified_time(&self) -> crate::Result<SystemTime>;
    fn as_path(&self) -> &Path;
    unsafe fn open_fd(&self) -> crate::Result<i32>;
    fn file_name_index(&self) -> u16;
    fn as_os_str(&self) -> &OsStr;
    fn exists(&self) -> bool;
    fn to_direntry<S>(&self) -> Result<DirEntry<S>>
    where
        S: BytesStorage;
    fn is_readable(&self) -> bool;
    fn is_writable(&self) -> bool;
    fn metadata(&self) -> crate::Result<std::fs::Metadata>;
    fn components(&self) -> impl Iterator<Item = &[u8]>;
    fn to_std_file_type(&self) -> crate::Result<std::fs::FileType>;
    fn as_str(&self) -> crate::Result<&str>;
    unsafe fn as_str_unchecked(&self) -> &str;
    fn to_string_lossy(&self) -> std::borrow::Cow<'_, str>;
    fn to_path(&self) -> PathBuf;
}

impl<T> BytePath<T> for T
where
    T: Deref<Target = [u8]>,
{
    #[inline]
    /// Converts a byte slice into a C string pointer
    /// Utilises `LOCAL_PATH_MAX` to create an upper bounded array
    /// if the signature is too confusing, use the `cstr!` macro instead.
    fn as_cstr_ptr<F, R, VT>(&self, f: F) -> R
    where
        F: FnOnce(*const VT) -> R,
        VT: ValueType, // VT==ValueType is u8/i8
    {
        debug_assert!(
            self.len() < crate::LOCAL_PATH_MAX, //delcared at compile time via env_var or default to 1024
            "Input too large for buffer"
        );

        let mut c_path_buf_start = crate::AlignedBuffer::<u8, {crate::LOCAL_PATH_MAX}>::new();

        let c_path_buf = c_path_buf_start.as_mut_ptr();

        // copy bytes using copy_nonoverlapping to avoid ub check
        unsafe {
            core::ptr::copy_nonoverlapping(self.as_ptr(), c_path_buf, self.len());
            c_path_buf.add(self.len()).write(0) // Null terminate the string
        };

        f(c_path_buf.cast::<_>())
    }

    /// Returns the extension of the file as a byte slice, if it exists.
    /// If the file has no '.' returns `None`.
    #[inline]
    fn extension(&self) -> Option<&[u8]> {
        unsafe{memrchr(b'.', self).map(|pos| self.get_unchecked(pos+1..))} //avoid UB check
        // # SAFETY
        // the filename is guaranteed to have more than one character at the end
    }

    /// Converts the byte slice into a `DirEntry`.
    /// This is a convenience method that allows you to create a `DirEntry` from a
    /// byte slice without needing to convert it to an `OsStr` first.
    #[inline]
    fn to_direntry<S>(&self) -> Result<DirEntry<S>>
    where
        S: BytesStorage,
    {
        // Convert the byte slice to an OsStr and then to a DirEntry

        DirEntry::<S>::new(self.as_os_str())
    }

    /// Converts the byte slice into a `PathBuf`.
    /// This is a simple conversion that does not check if the path is valid.
    fn to_path(&self) -> PathBuf {
        // Convert the byte slice to a PathBuf
        self.as_os_str().into()
    }

    /// Checks if the file matches the given extension.
    /// Returns `true` if the file's extension matches, `false` otherwise.
    #[inline]
    fn matches_extension(&self, ext: &[u8]) -> bool {
        self.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    /// Returns the size of the file in bytes.
    /// If the file size cannot be determined, returns `InvalidStat`
    #[inline]
    #[allow(clippy::cast_sign_loss)] //it's safe to cast here because we're dealing with file sizes which are always positive
    fn size(&self) -> crate::Result<u64> {
        self.get_stat().map(|s| s.st_size as u64)
    }
    #[inline]
    unsafe fn open_fd(&self) -> crate::Result<i32> {
        // Opens the file and returns a file descriptor.
        // This is a low-level operation that may fail if the file does not exist or cannot be opened.
        const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK;
        self.as_cstr_ptr(|ptr| {
            let fd = unsafe { libc::open(ptr, FLAGS) };

            if fd < 0i32 {
                Err(std::io::Error::last_os_error().into())
            } else {
                Ok(fd)
            }
        })
    }

    #[inline]
    /// Converts into `libc::stat` or returns `DirEntryError::InvalidStat`
    /// More specialised errors are on the TODO list.
    fn get_stat(&self) -> crate::Result<stat> {
        let mut stat_buf = MaybeUninit::<stat>::uninit();
        let res = self.as_cstr_ptr(|ptr| unsafe { lstat(ptr, stat_buf.as_mut_ptr()) });

        if res == 0 {
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }
    /// Get last modification time, this will be more useful when I implement filters for it.
    #[inline]
    #[allow(clippy::cast_possible_truncation)] //it's fine here because i32 is  plenty
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    #[allow(clippy::map_err_ignore)] //specify these later TODO!
    fn modified_time(&self) -> crate::Result<SystemTime> {
         
        let s = self.get_stat()?;
        let modified_time=access_stat!(s,st_mtime);
        let modified_seconds=access_stat!(s,st_mtime_nsec);
        crate::unix_time_to_system_time(modified_time, modified_seconds)
            .map_err(|_| crate::DirEntryError::TimeError)
    }



    /// Converts the byte slice into a `Path`.
    #[inline]
    fn as_path(&self) -> &Path {
        //&[u8] <=> &OsStr <=> &Path on linux
        self.as_os_str().as_ref()
    }

    #[inline]
    #[allow(clippy::transmute_ptr_to_ptr)]
    ///cheap conversion from byte slice to `OsStr`
    fn as_os_str(&self) -> &OsStr {
        //same represensation fuck clippy  yapping
        unsafe { transmute::<&[u8], &OsStr>(self) }
    }

    #[inline]
    ///somewhatcostly check for readable files
    fn is_readable(&self) -> bool {
        unsafe { self.as_cstr_ptr(|ptr| access(ptr, R_OK)) == 0i32 }
    }

    #[inline]
    ///somewhat costly check for writable files(by current user)
    fn is_writable(&self) -> bool {
        //maybe i can automatically exclude certain files from this check to
        //then reduce my syscall total, would need to read into some documentation. zadrot ebaniy
        unsafe { self.as_cstr_ptr(|ptr| access(ptr, W_OK)) == 0i32 }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::map_err_ignore)] //specify these later TODO
    ///returns the std definition of metadata for easy validation/whatever purposes.
    fn metadata(&self) -> crate::Result<std::fs::Metadata> {
        std::fs::metadata(self.as_os_str()).map_err(|_| crate::DirEntryError::MetadataError) //TODO! provide a more specialised error
    }

    #[inline]
    // Returns an iterator over the components of the path.
    /// This splits the path by '/' and filters out empty components.
    fn components(&self) -> impl Iterator<Item = &[u8]> {
        self.split(|&b| b == b'/').filter(|s| !s.is_empty())
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::map_err_ignore)] //specify these later TODO!
    ///Costly conversion to a `std::fs::FileType`
    fn to_std_file_type(&self) -> crate::Result<std::fs::FileType> {
        //  can't directly create a std::fs::FileType,
        // we need to make a system call to get it
        std::fs::symlink_metadata(self.as_path())
            .map(|m| m.file_type())
            .map_err(|_| crate::DirEntryError::MetadataError)
    }

    #[inline]
    ///checks if the file exists, this, makes a syscall
    fn exists(&self) -> bool {
        unsafe { self.as_cstr_ptr(|ptr| access(ptr, F_OK)) == 0 }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::missing_const_for_fn)] //this cant be const clippy be LYING
    ///returns the path as a &str
    ///this is safe because path is always valid utf8
    ///(because unix paths are always valid utf8)
    fn as_str(&self) -> crate::Result<&str> {
        core::str::from_utf8(self).map_err(crate::DirEntryError::Utf8Error)
    }

    #[inline]
    #[allow(clippy::missing_const_for_fn)]
    /// Returns the path as a &str without checking if it is valid UTF-8.
    /// # Safety
    /// The caller must ensure that the bytes in `self.path` form valid UTF-8.
    #[allow(clippy::missing_panics_doc)]
    unsafe fn as_str_unchecked(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(self) }
    }

    #[inline]
    ///Returns the path as a    `Cow<str>`
    fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(self)
    }

    /// Get the length of the basename of a path (up to and including the last '/')
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    fn file_name_index(&self) -> u16 {
        memrchr(b'/', self).map_or(1, |pos| (pos + 1) as _)
    }
}

impl<S> fmt::Display for DirEntry<S>
where
    S: BytesStorage,
{
    //i might need to change this to show other metadata.
    #[inline]
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
        entry.to_path()
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
#[allow(clippy::cast_possible_truncation)] //no truncation issue (reclen is always under u16, casting to and  from a usize is lossless)
pub trait DirentConstructor<S: BytesStorage> {
    // Required accessors
    fn path_buffer(&mut self) -> &mut PathBuffer;
    fn file_index(&self) -> usize; //modify name a bit so we dont get collisions.
    fn parent_depth(&self) -> u8;

    #[inline]
    unsafe fn construct_entry(&mut self, drnt: *const dirent64) -> DirEntry<S> {
        let base_len = self.file_index();
        let full_path = crate::utils::construct_path(self.path_buffer(), base_len, drnt);

        let dtype = unsafe { offset_dirent!(drnt, d_type) };
        let inode = unsafe { offset_dirent!(drnt, d_ino) };

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
        self.file_name_index as usize
    }

    #[inline]
    fn parent_depth(&self) -> u8 {
        self.parent_depth
    }
}

#[cfg(target_os = "linux")]
impl<S: BytesStorage> DirentConstructor<S> for crate::direntry::DirEntryIterator<S> {
    #[inline]
    fn path_buffer(&mut self) -> &mut crate::PathBuffer {
        &mut self.path_buffer
    }

    #[inline]
    fn file_index(&self) -> usize {
        self.file_name_index as usize
    }

    #[inline]
    fn parent_depth(&self) -> u8 {
        self.parent_depth
    }
}
