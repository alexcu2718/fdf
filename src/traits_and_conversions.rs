#![allow(clippy::missing_safety_doc)]
use crate::buffer::ValueType;
use libc::{F_OK, R_OK, W_OK, access, lstat, stat};
use std::ffi::OsStr;
use std::mem::MaybeUninit;
use std::mem::transmute;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
///a trait over anything which derefs to `&[u8]` then convert to *const i8 or *const u8 (inferred ), useful for FFI.
pub trait BytePath<T> {
    fn as_cstr_ptr<F, R, VT>(&self, f: F) -> R
    where
        F: FnOnce(*const VT) -> R,
        VT: ValueType; // VT==ValueType is u8/i8

    fn extension(&self) -> Option<&[u8]>
    where
        T: Deref<Target = [u8]>;
    fn matches_extension(&self, ext: &[u8]) -> bool
    where
        T: Deref<Target = [u8]>;
    unsafe fn size(&self) -> crate::Result<u64>
    where
        T: Deref<Target = [u8]>;
    fn get_stat(&self) -> crate::Result<stat>
    where
        T: Deref<Target = [u8]>;
    fn modified_time(&self) -> crate::Result<SystemTime>
    where
        T: Deref<Target = [u8]>;
    fn as_path(&self) -> &Path
    where
        T: Deref<Target = [u8]>;
    fn as_os_str(&self) -> &OsStr
    where
        T: Deref<Target = [u8]>;
    fn exists(&self) -> bool
    where
        T: Deref<Target = [u8]>;
    fn is_readable(&self) -> bool
    where
        T: Deref<Target = [u8]>;
    fn is_writable(&self) -> bool
    where
        T: Deref<Target = [u8]>;
    fn metadata(&self) -> crate::Result<std::fs::Metadata>
    where
        T: Deref<Target = [u8]>;
    fn components(&self) -> impl Iterator<Item = &[u8]>
    where
        T: Deref<Target = [u8]>;
    fn to_std_file_type(&self) -> crate::Result<std::fs::FileType>
    where
        T: Deref<Target = [u8]>;
    fn as_str(&self) -> crate::Result<&str>
    where
        T: Deref<Target = [u8]>;
    unsafe fn as_str_unchecked(&self) -> &str
    where
        T: Deref<Target = [u8]>;
    fn to_string_lossy(&self) -> std::borrow::Cow<'_, str>
    where
        T: Deref<Target = [u8]>;
    fn is_absolute(&self) -> bool
    where
        T: Deref<Target = [u8]>;
    fn is_relative(&self) -> bool
    where
        T: Deref<Target = [u8]>;
    fn to_path(&self) -> PathBuf
    where
        T: Deref<Target = [u8]>,
    {
        self.as_path().to_owned()
    }
    fn realpath(&self) -> crate::Result<&[u8]>
    where
        T: Deref<Target = [u8]>;
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
            self.len() < crate::LOCAL_PATH_MAX,
            "Input too large for buffer"
        );

        let c_path_buf = crate::PathBuffer::new().as_mut_ptr();

        // copy bytes using copy_nonoverlapping to avoid ub check
        unsafe {
            std::ptr::copy_nonoverlapping(self.as_ptr(), c_path_buf, self.len());
            c_path_buf.add(self.len()).write(0); // Null terminate the string
        }

        f(c_path_buf.cast::<_>())
    }

    /// Returns the extension of the file as a byte slice, if it exists.
    /// If the file has no extension, returns `None`.
    #[inline]
    fn extension(&self) -> Option<&[u8]> {
        self.rsplit(|&b| b == b'.').next()
    }

    /// Checks if the file matches the given extension.
    /// Returns `true` if the file's extension matches, `false` otherwise.
    #[inline]
    fn matches_extension(&self, ext: &[u8]) -> bool {
        self.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    /// Returns the size of the file in bytes.
    /// If the file size cannot be determined, returns 0.
    #[inline]
    unsafe fn size(&self) -> crate::Result<u64> {
        self.get_stat().map(|s| s.st_size as u64)
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
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    fn modified_time(&self) -> crate::Result<SystemTime> {
        self.get_stat().and_then(|s| {
            crate::unix_time_to_system_time(s.st_mtime, s.st_mtime_nsec as i32)
                .map_err(|_| crate::DirEntryError::TimeError)
        })
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
        unsafe { self.as_cstr_ptr(|ptr| access(ptr, R_OK)) == 0 }
    }

    #[inline]
    ///somewhat costly check for writable files(by current user)
    fn is_writable(&self) -> bool {
        //maybe i can automatically exclude certain files from this check to
        //then reduce my syscall total, would need to read into some documentation. zadrot ebaniy
        unsafe { self.as_cstr_ptr(|ptr| access(ptr, W_OK)) == 0 }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    ///returns the std definition of metadata for easy validation/whatever purposes.
    fn metadata(&self) -> crate::Result<std::fs::Metadata> {
        std::fs::metadata(self.as_os_str()).map_err(|_| crate::DirEntryError::MetadataError)
    }

    #[inline]
    // Returns an iterator over the components of the path.
    /// This splits the path by '/' and filters out empty components.
    fn components(&self) -> impl Iterator<Item = &[u8]> {
        self.split(|&b| b == b'/').filter(|s| !s.is_empty())
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
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
        std::str::from_utf8(self).map_err(crate::DirEntryError::Utf8Error)
    }

    #[inline]
    #[allow(clippy::missing_const_for_fn)]
    /// Returns the path as a &str without checking if it is valid UTF-8.
    /// # Safety
    /// The caller must ensure that the bytes in `self.path` form valid UTF-8.
    #[allow(clippy::missing_panics_doc)]
    unsafe fn as_str_unchecked(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(self) }
    }

    #[inline]
    ///Returns the path as a    `Cow<str>`
    fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(self)
    }

    #[inline]
    ///checks if the path is absolute,
    fn is_absolute(&self) -> bool {
        self[0] == b'/'
    }

    #[inline]
    ///checks if the path is relative,
    fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    ///resolves the path to an absolute path
    /// this is a costly operation, as it requires a syscall to resolve the path.
    /// unless the path is already absolute, in which case its a trivial operation
    fn realpath(&self) -> crate::Result<&[u8]> {
        if self.is_absolute() {
            return Ok(self);
        }
        //cast byte slice into a *const c_char/i8 pointer with a null terminator THEN pass it to realpath along with a null mut pointer
        let ptr = unsafe {
            self.as_cstr_ptr(|cstrpointer| libc::realpath(cstrpointer, std::ptr::null_mut()))
        };
        if ptr.is_null() {
            //check for null
            return Err(std::io::Error::last_os_error().into());
        }
        //better to use strlen here because path is likely to be too long to benefit from repne scasb
        //we also use `std::ptr::slice_from_raw_parts`` to  avoid a UB check (trivial but we're leaving safety to user :)))))))))))
        Ok(unsafe { &*std::ptr::slice_from_raw_parts(ptr.cast(), libc::strlen(ptr)) })
    }
}

pub trait PathAsBytes {
    fn as_bytes(&self) -> &[u8];
}

#[allow(clippy::transmute_ptr_to_ptr)]
impl PathAsBytes for Path {
    #[inline]
    fn as_bytes(&self) -> &[u8] {
        //&[u8] <=> &OsStr <=> &Path on linux
        unsafe { transmute::<&Self, _>(self) }
    }
}
