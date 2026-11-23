use crate::{Unique, dirent64, fs::Result, readdir64};
use core::ptr::NonNull;
use libc::{DIR, closedir, fdopendir};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, IntoRawFd as _, OwnedFd, RawFd};

#[derive(Debug)]
pub struct OwnedDir<'fd> {
    inner: NonNull<DIR>, // Exclusive only access
    // We store this but just need it for deref,
    // We also don't want a non-inlineable glibc call to get the fd. (even if it's just pointer access)
    fdes: BorrowedFd<'fd>,
}

impl AsFd for OwnedDir<'_> {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fdes.as_fd()
    }
}

impl AsRawFd for OwnedDir<'_> {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fdes.as_raw_fd()
    }
}

impl Drop for OwnedDir<'_> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: `inner` is a valid DIR* from a successful `fdopendir`, closed only here.
        let retval = unsafe { closedir(self.inner.as_ptr()) };
        debug_assert_eq!(retval, 0, "not closed properly!");
    }
}

impl OwnedDir<'_> {
    /// Wraps an owned file descriptor in a directory stream.
    ///
    /// # Errors
    /// Returns the OS error if `fdopendir` fails (e.g. the fd is not a directory).
    #[inline]
    pub fn from_owned_fd(fd: OwnedFd) -> Result<Self> {
        // SAFETY: `fd` is a valid, open file descriptor owned by us.
        let ret = unsafe { fdopendir(fd.as_raw_fd()) };

        match NonNull::new(ret) {
            Some(valid) => {
                // Ownership moves to the DIR stream, which closes the fd in `closedir`.
                let raw = fd.into_raw_fd();
                // SAFETY: `raw` is open and stays valid until `closedir` in `Drop`.
                let fdes = unsafe { BorrowedFd::borrow_raw(raw) };
                Ok(Self { inner: valid, fdes })
            }
            None => return_os_error!(),
        }
    }

    #[inline]
    /// Gets the next pointer from `readdir64`
    pub fn next_pointer(&mut self) -> Option<Unique<dirent64>> {
        // SAFETY: passing a valid pointer, no other access is possible to this handle
        unsafe { Unique::new(readdir64(self.inner.as_ptr())) }
    }
}
