use crate::buffer::ValueType;
use libc::{lstat, stat};
use std::ffi::OsStr;
use std::mem::MaybeUninit;
use std::mem::transmute;
use std::ops::Deref;
use std::path::Path;
///a trait over anything which derefs to `&[u8]` then convert to *const i8 or *const u8 (inferred ), useful for FFI.
pub trait BytesToCstrPointer<T> {
    fn as_cstr_ptr<F, R, VT>(&self, f: F) -> R
    where
        F: FnOnce(*const VT) -> R,
        VT: ValueType; // VT==ValueType is u8/i8
}

impl<T> BytesToCstrPointer<T> for T
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
}


pub trait GetExtension{

    fn extension(&self)->Option<&[u8]>;
}


impl<T> GetExtension for T
where
    T: Deref<Target = [u8]>,
{
    /// Returns the extension of the file as a byte slice, if it exists.
    /// If the file has no extension, returns `None`.
    #[inline]
    fn extension(&self) -> Option<&[u8]> {
        self.rsplit(|&b| b == b'.').next()
       
    }

}



pub trait MatchesExtension{
    fn matches_extension(&self, ext: &[u8]) -> bool;

}



impl<T> MatchesExtension for T
where
    T: Deref<Target = [u8]>,
{
    /// Checks if the file matches the given extension.
    /// Returns `true` if the file's extension matches, `false` otherwise.
   
    #[inline]
    fn matches_extension(&self, ext: &[u8]) -> bool {
        self.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }
}


pub trait GetSize {
    /// Returns the size of the file in bytes.
    unsafe fn size(&self) -> crate::Result<u64>;
}

impl <T> GetSize for T
where
    T: Deref<Target = [u8]>,
{
    /// Returns the size of the file in bytes.
    /// If the file size cannot be determined, returns 0.
    #[inline]
    unsafe fn size(&self) -> crate::Result<u64> {
         self.get_stat().map(|s| s.st_size as u64)
    }
}






#[allow(dead_code)]
pub(crate) trait AsOsStr {
    fn as_os_str(&self) -> &OsStr;
}

impl<T> AsOsStr for T
where
    T: Deref<Target = [u8]>,
{
    #[inline]
    #[allow(clippy::transmute_ptr_to_ptr)]
    ///cheap conversion from byte slice to `OsStr`
    fn as_os_str(&self) -> &OsStr {
        //same represensation fuck clippy  yapping
        unsafe { transmute::<&[u8], &OsStr>(self) }
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

pub trait ToStat {
    ///Converts the type into `libc::stat`, this is used internally to get file metadata.
    #[allow(clippy::missing_errors_doc)] //SKIPPING ERRORS UNTIL DONE.
    fn get_stat(&self) -> crate::Result<stat>;
}

impl<T> ToStat for T
where
    T: Deref<Target = [u8]>,
{
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
}
