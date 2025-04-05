use std::os::unix::ffi::OsStrExt;

pub type Result<T> = std::result::Result<T, crate::DirEntryError>;

#[derive(Clone, Debug)]
pub struct OsBytes {
    pub(crate) bytes: slimmer_box::SlimmerBox<[u8], u16>,
} //10 bytes,this is basically a box with a much thinner pointer, it's 10 bytes instead of 16.

impl OsBytes {
    #[inline]
    #[must_use]
    pub fn new(bytes: &[u8]) -> Self {
        unsafe {
            Self {
                bytes: slimmer_box::SlimmerBox::new_unchecked(bytes),
            }
        }
    }
    #[inline]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[inline]
    #[must_use]
    pub fn as_os_str(&self) -> &std::ffi::OsStr {
        std::ffi::OsStr::from_bytes(self.as_bytes())
    }
}

impl<T: AsRef<[u8]>> From<T> for OsBytes {
    #[inline]
    #[must_use]
    fn from(data: T) -> Self {
        Self::new(data.as_ref())
    }
}
