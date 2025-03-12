pub trait PointerUtils {
    fn as_cstr_ptr<F, R>(&self, f: F) -> R
    where
        F: FnOnce(*const libc::c_char) -> R;
}

impl PointerUtils for [u8] {
    #[inline(always)]
    #[allow(clippy::inline_always)]
    ///converts a byte slice into a c str(ing) pointer
    ///utilises `PATH_MAX` (4096 BYTES) to create an upper bounded array
    //needs to be done as a callback because we need to keep the reference to the array
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
