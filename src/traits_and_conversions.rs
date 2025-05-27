use crate::{LOCAL_PATH_MAX,Result};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::mem::MaybeUninit;
use std::path::Path;
use libc::{lstat,stat};

pub trait BytesToCstrPointer {
    fn as_cstr_ptr<T,F, R>(&self, f: F) -> R
    where
        F: FnOnce(*const T) -> R;
}
//convenience thing for me suck it
pub trait ToOsStr {
    fn to_os_str(&self) -> &OsStr;
}

impl ToOsStr for [u8] {
    fn to_os_str(&self) -> &OsStr {
        OsStrExt::from_bytes(self)
    }
}

impl BytesToCstrPointer for [u8] {
    #[inline]
    fn as_cstr_ptr<T,F, R>(&self, f: F) -> R
    where
        F: FnOnce(*const T) -> R,
    {
        debug_assert!(
            self.len() < LOCAL_PATH_MAX,
            "Input too large for buffer"
        );

        // //initialise  the buffer (this doesn't actually create the memory)
        let mut c_path_buf = MaybeUninit::<[u8; LOCAL_PATH_MAX]>::uninit();

        unsafe {
            // Get a raw pointer to the buffer's data
            let buf_ptr = c_path_buf.as_mut_ptr() as *mut u8;
            
            // Copy the slice into the buffer
            std::ptr::copy_nonoverlapping(
                self.as_ptr(),
                buf_ptr,
                self.len(),
            );
            
            // Add null terminator
            buf_ptr.add(self.len()).write(0);
            
            // Call the callback with the pointer to the null-terminated string
            f(buf_ptr as *const T)
        }
    }
}




pub trait PathToBytes {
    fn to_bytes(&self) -> &[u8];
}

impl PathToBytes for Path {
    #[inline]
    fn to_bytes(&self) -> &[u8] {
        OsStrExt::as_bytes(self.as_os_str())
    }
}

pub trait ToStat {
    #[allow(clippy::missing_errors_doc)] //SKIPPING ERRORS UNTIL DONE.
    fn get_stat(&self) -> Result<stat>;
}

impl ToStat for crate::DirEntry {
    ///Converts into `libc::stat` , mostly for internal use..probably...
    #[inline]
    fn get_stat(&self) -> Result<stat> {
        let mut stat_buf = MaybeUninit::<stat>::uninit();

        let res = self
            .as_cstr_ptr(|ptr| unsafe { lstat(ptr, stat_buf.as_mut_ptr()) });

        if res == 0 {
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }
}

impl ToStat for &[u8] {
    #[inline]
    ///Converts a byte slice into  `libc::stat`, internal probably.
    fn get_stat(&self) -> Result<stat> {
        let mut stat_buf = MaybeUninit::<stat>::uninit();
        let res = self
            .as_cstr_ptr(|ptr| unsafe { lstat(ptr, stat_buf.as_mut_ptr()) });

        if res == 0 {
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }
}
