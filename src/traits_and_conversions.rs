use libc::{lstat, stat};
use std::ffi::OsStr;
use std::mem::MaybeUninit;
use std::mem::transmute;
use std::path::Path;
use std::ops::Deref;
use crate::buffer::ValueType;

pub trait BytesToCstrPointer<T> {
    fn as_cstr_ptr<F, R, IT>(&self, f: F) -> R
    where
        F: FnOnce(*const IT) -> R,
        IT: ValueType; //IT is u8/i8
}

impl <T> BytesToCstrPointer<T> for T
where T:Deref<Target=[u8]>, {
    #[inline]
    /// Converts a byte slice into a C string pointer
    /// Utilises `LOCAL_PATH_MAX` to create an upper bounded array
    /// if the signature is too confusing, use the `cstr!` macro instead.
    fn as_cstr_ptr<F, R, IT>(&self, f: F) -> R
    where
        F: FnOnce(*const IT) -> R,
        IT: ValueType, // IT is u8/i8
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

pub trait AsOsStr {
    fn as_os_str(&self) -> &OsStr;
}

impl AsOsStr for [u8] {
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
        let res = self.as_cstr_ptr(|ptr| unsafe {
            lstat(ptr, stat_buf.as_mut_ptr())
        });

        if res == 0 {
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }
}

