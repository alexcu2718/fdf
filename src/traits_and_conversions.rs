pub trait BytesToCstrPointer {
    fn as_cstr_ptr<F, R>(&self, f: F) -> R
    where
        F: FnOnce(*const libc::c_char) -> R;
}
//convenience thing for me suck it
pub trait ToOsStr {
    fn to_os_str(&self) -> &std::ffi::OsStr;
}

impl ToOsStr for [u8] {
    fn to_os_str(&self) -> &std::ffi::OsStr {
        std::os::unix::ffi::OsStrExt::from_bytes(self)
    }
}

impl BytesToCstrPointer for [u8] {
    #[inline]
    ///converts a byte slice into a c str(ing) pointer
    ///utilises `LOCAL_PATH_MAX` converts a file of up to 512 birts to create an upper bounded array
    //needs to be done as a callback because we need to keep the reference to the array
    //apparently this can fuck up on some weird filesystems, like NTFS(`PATH_MAX` ) being incorrect.
    fn as_cstr_ptr<F, R>(&self, f: F) -> R
    where
        F: FnOnce(*const i8) -> R,
    {
        let mut c_path_buf = [0u8; crate::LOCAL_PATH_MAX];
        c_path_buf[..self.len()].copy_from_slice(self);
        // null terminate the string
        c_path_buf[self.len()] = 0;
        f(std::ptr::addr_of!(c_path_buf).cast::<i8>())
        //if you thought nested macros were  a pain then hello
    }
}

pub trait PathToBytes {
    fn to_bytes(&self) -> &[u8];
}

impl PathToBytes for std::path::Path {
    #[inline]
    fn to_bytes(&self) -> &[u8] {
        std::os::unix::ffi::OsStrExt::as_bytes(self.as_os_str())
    }
}

pub trait ToStat {
    #[allow(clippy::missing_errors_doc)] //SKIPPING ERRORS UNTIL DONE.
    fn get_stat(&self) -> crate::Result<libc::stat>;
}

impl ToStat for crate::DirEntry {
    ///Converts into `libc::stat` , mostly for internal use..probably...
    #[inline]
    fn get_stat(&self) -> crate::Result<libc::stat> {
        let mut stat_buf = std::mem::MaybeUninit::<libc::stat>::uninit();

        let res = self
            .as_cstr_ptr(|ptr| unsafe { libc::stat(ptr, std::ptr::addr_of_mut!(stat_buf).cast()) });

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
    fn get_stat(&self) -> crate::Result<libc::stat> {
        let mut stat_buf = std::mem::MaybeUninit::<libc::stat>::uninit();
        let res = self
            .as_cstr_ptr(|ptr| unsafe { libc::stat(ptr, std::ptr::addr_of_mut!(stat_buf).cast()) });

        if res == 0 {
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }
}
