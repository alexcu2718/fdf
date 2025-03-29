use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;


//this is essentially for a cheat i wish to try to save on heap allocations, ignore this
pub struct ByteArray{ 
    path:[u8; libc::PATH_MAX as usize] ,
    path_len:usize,
}
impl ByteArray{

        #[inline]
        #[must_use]
        pub fn new(full_path:&[u8]) -> Self {
            let full_path_len = full_path.len();
            let mut c_path_buf = [0u8; libc::PATH_MAX as usize];
            c_path_buf[..full_path_len].copy_from_slice(full_path);
            // null terminate the string
            c_path_buf[full_path_len] = 0;
            Self{path:c_path_buf, path_len:full_path_len}
        }

        #[inline]
        #[must_use]
        pub fn as_path(&self) -> &[u8] {
            &self.path[..self.path_len]
        }


}

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


pub trait PathToBytes{
    fn to_bytes(&self)->&[u8];
}

impl PathToBytes for Path{
    #[inline(always)]
    #[allow(clippy::inline_always)]
    fn to_bytes(&self)->&[u8] {
        self.as_os_str().as_bytes()
    }
}

