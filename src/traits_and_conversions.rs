use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::DirEntry;

//this is essentially for a cheat i wish to try to save on heap allocations, ignore this


pub trait BytesToCstrPointer {
    fn as_cstr_ptr<F, R>(&self, f: F) -> R
    where
        F: FnOnce(*const libc::c_char) -> R;
}

pub trait ToOsStr {
    fn to_os_str(&self) -> &OsStr;
}

impl ToOsStr for [u8] {
    fn to_os_str(&self) -> &OsStr {
        OsStr::from_bytes(self)
    }
}

impl BytesToCstrPointer for [u8] {
    #[inline(always)]
    #[allow(clippy::inline_always)]
    ///converts a byte slice into a c str(ing) pointer
    ///utilises `PATH_MAX` (4096 BITS/256 BYTES) to create an upper bounded array
    //needs to be done as a callback because we need to keep the reference to the array
    //apparently this can fuck up on some weird filesystems, like NTFS(`PATH_MAX` ) being incorrect.
    fn as_cstr_ptr<F, R>(&self, f: F) -> R
    where
        F: FnOnce(*const libc::c_char) -> R,
    {
        let mut c_path_buf = [0u8; libc::PATH_MAX as usize];
        c_path_buf[..self.len()].copy_from_slice(self);
        // null terminate the string
        c_path_buf[self.len()] = 0;
        f(c_path_buf.as_ptr().cast())
    }
}

pub trait PathToBytes {
    fn to_bytes(&self) -> &[u8];
}

impl PathToBytes for Path {
    #[inline(always)]
    #[allow(clippy::inline_always)]
    fn to_bytes(&self) -> &[u8] {
        self.as_os_str().as_bytes()
    }
}




pub(crate) trait ToStat{
    fn get_stat(&self) -> crate::Result<libc::stat>;
}

impl ToStat for DirEntry{

    fn get_stat(&self) -> crate::Result<libc::stat> {
        let mut stat_buf = std::mem::MaybeUninit::<libc::stat>::uninit();

        let res = self.as_cstr_ptr(|ptr| unsafe { libc::stat(ptr, stat_buf.as_mut_ptr()) });
    
        if res == 0 {
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }

}

impl ToStat for &[u8] {
    #[inline(always)]
    #[allow(clippy::inline_always)]
    ///Converts a byte slice into a pointer to libc::stat pointer
    fn get_stat(&self) -> crate::Result<libc::stat> {
        let mut stat_buf = std::mem::MaybeUninit::<libc::stat>::uninit();

        let res = self.as_cstr_ptr(|ptr| unsafe { libc::stat(ptr, stat_buf.as_mut_ptr()) });
    
        if res == 0 {
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }
}
