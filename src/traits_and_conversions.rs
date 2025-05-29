






use std::path::Path;
use libc::{stat,lstat};
use std::mem::transmute;
use std::mem::MaybeUninit;
use std::ffi::OsStr;


pub trait BytesToCstrPointer {
    fn as_cstr_ptr<F, R>(&self, f: F) -> R
    where
        F: FnOnce(*const i8) -> R;
}
//convenience thing for me suck it
pub trait AsOsStr {
    fn as_os_str(&self) -> &OsStr;
}

impl AsOsStr for [u8] {
    #[inline]
    #[allow(clippy::transmute_ptr_to_ptr)]
    fn as_os_str(&self) -> &OsStr {
        //same represensation fuck clippy  yapping
        unsafe { transmute::<&[u8], &OsStr>(self) }
    }
}
















impl BytesToCstrPointer for [u8] {
    #[inline]
    /// Converts a byte slice into a C string pointer
    /// Utilizes `LOCAL_PATH_MAX` to create an upper bounded array
    fn as_cstr_ptr<F, R>(&self, f: F) -> R
    where
        F: FnOnce(*const i8) -> R,
    {
        debug_assert!(
            self.len() < crate::LOCAL_PATH_MAX,
            "Input too large for buffer"
        );

        let c_path_buf = crate::PathBuffer::new().as_mut_ptr();

        // Copy bytes using copy_nonoverlapping
        unsafe {
            std::ptr::copy_nonoverlapping(self.as_ptr(),c_path_buf, self.len());
            c_path_buf.add(self.len()).write(0); // Null terminate the string

        }

      
   

        f(c_path_buf.cast::<_>())
    }
}




pub trait PathAsBytes {
    fn as_bytes(&self) -> &[u8];
}

#[allow(clippy::transmute_ptr_to_ptr)]
impl PathAsBytes for Path {
    #[inline]
    fn as_bytes(&self) -> &[u8] {
        unsafe{transmute::<&Self,_>(self)}
    }
}

pub trait ToStat {
    #[allow(clippy::missing_errors_doc)] //SKIPPING ERRORS UNTIL DONE.
    fn get_stat(&self) -> crate::Result<stat>;
}

impl ToStat for crate::DirEntry {
    ///Converts into `libc::stat` , mostly for internal use..probably...
    #[inline]
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

impl ToStat for &[u8] {
    #[inline]
    ///Converts a byte slice into  `libc::stat`, internal probably.
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
