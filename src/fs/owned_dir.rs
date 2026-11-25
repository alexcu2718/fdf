use crate::{Unique, dirent64, fs::Result, readdir64};
#[allow(unused)]
use core::marker::PhantomData;
use core::ptr::NonNull;
#[allow(unused)]
use libc::{DIR, closedir, fdopendir, off_t};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, IntoRawFd, OwnedFd, RawFd};

#[cfg(has_faster_readdir)]
use crate::fs::SyscallBuffer;

#[derive(Debug)]
pub struct OwnedDir<'fd> {
    dirp: NonNull<DIR>, // Exclusive only access
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
        // SAFETY: `dirp` is a valid DIR* from a successful `fdopendir`, closed only here.
        let retval = unsafe { closedir(self.as_mut_ptr()) };
        debug_assert_eq!(retval, 0, "not closed properly!");
    }
}

impl TryFrom<OwnedFd> for OwnedDir<'_> {
    type Error = crate::DirEntryError;

    /// Wraps an owned file descriptor in a directory stream.
    ///
    /// # Errors
    /// Returns the OS error if `fdopendir` fails (e.g. the fd is not a directory).
    #[inline]
    fn try_from(fd: OwnedFd) -> Result<Self> {
        Self::from_owned_fd(fd)
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
        let ret: *mut DIR = unsafe { fdopendir(fd.as_raw_fd()) };

        match NonNull::new(ret) {
            Some(inner) => {
                // Ownership moves to the DIR stream, which closes the fd in `closedir`.
                let raw = fd.into_raw_fd();
                // strong drop order.
                // SAFETY: `raw` is open and stays valid until `closedir` in `Drop`.
                let fdes = unsafe { BorrowedFd::borrow_raw(raw) };
                Ok(Self { dirp: inner, fdes })
            }
            None => return_os_error!(),
        }
    }

    #[inline]
    #[expect(clippy::needless_pass_by_ref_mut, reason = "interior mutability")]
    const fn as_mut_ptr(&mut self) -> *mut DIR {
        self.dirp.as_ptr()
    }

    #[inline]
    /// Gets the next pointer from `readdir64`
    pub fn next_pointer(&mut self) -> Option<Unique<dirent64>> {
        // SAFETY: passing a valid pointer, no other access is possible to this handle
        unsafe { Unique::new(readdir64(self.as_mut_ptr())) }
    }
}

/// An owned directory file descriptor that buffers batched `getdents`/`getdirentries64` reads,
/// mirroring `readdir` semantics (all entries are returned, including `.` and `..`).
///
/// Backs [`crate::fs::GetDents`].
#[cfg(has_faster_readdir)]
#[derive(Debug)]
pub struct OwnedFdDir<'fdes> {
    fd: OwnedFd,
    syscall_buffer: SyscallBuffer,
    /// Current read position within `syscall_buffer`
    offset: usize,
    /// Number of valid bytes in `syscall_buffer` from the last syscall
    remaining_bytes: usize,
    /// Set once the kernel reports no more entries
    end_of_stream: bool,
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    /// The base pointer (seek cookie) for the `getdirentries64` call
    base_pointer: off_t,
    /// Ties `self` to the lifetime of the fd's origin, mirroring `OwnedDir<'fd>`.
    _marker: PhantomData<BorrowedFd<'fdes>>,
}

#[cfg(has_faster_readdir)] // can't exist for `OwnedDir`
impl IntoRawFd for OwnedFdDir<'_> {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}

#[cfg(has_faster_readdir)]
impl AsFd for OwnedFdDir<'_> {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

#[cfg(has_faster_readdir)]
impl AsRawFd for OwnedFdDir<'_> {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

#[cfg(has_faster_readdir)]
impl From<OwnedFd> for OwnedFdDir<'_> {
    #[inline]
    fn from(fd: OwnedFd) -> Self {
        Self::new(fd)
    }
}

#[cfg(has_faster_readdir)]
impl OwnedFdDir<'_> {
    pub(crate) const BUFFER_SIZE: usize = SyscallBuffer::BUFFER_SIZE;

    #[inline]
    pub(crate) const fn new(fd: OwnedFd) -> Self {
        Self {
            fd,
            syscall_buffer: SyscallBuffer::new(),
            offset: 0,
            remaining_bytes: 0,
            end_of_stream: false,
            #[cfg(any(target_os = "macos", target_os = "freebsd"))]
            base_pointer: 0,
            _marker: PhantomData,
        }
    }

    #[inline]
    #[must_use]
    /// Returns a boolean indicating whether the stream is exhausted
    pub const fn is_end_of_stream(&self) -> bool {
        self.end_of_stream
    }

    /**
     Refills the internal directory-entry buffer using the platform directory syscall.

     On Linux-like targets this dispatches to `getdents`, while on macOS and
     FreeBSD it calls `getdirentries64` and updates the internal base pointer
     required by that API.

     # Returns

     Returns the raw syscall result as a signed byte count:
     - positive: the number of bytes written into the buffer
     - `0`: end of directory stream
     - negative: the syscall failed

     This method does not interpret the returned entries; higher-level iteration
     code is responsible for consuming the buffer and handling end-of-stream.
    */
    #[inline] //TODO MAKE PUBLIC?
    pub(crate) fn getdents(&mut self) -> isize {
        #[cfg(not(any(target_os = "macos", target_os = "freebsd")))]
        {
            self.syscall_buffer.getdents(self.fd.as_fd())
        }
        #[cfg(any(target_os = "macos", target_os = "freebsd"))]
        {
            // SAFETY: passing a valid buffer to an open file descriptor and base pointer
            unsafe {
                self.syscall_buffer
                    .getdirentries64(self.inner.as_fd(), &mut self.base_pointer)
            }
        }
    }

    /// Refills the buffer, returns whether any bytes were read.
    /// See `GetDents::are_more_entries_remaining` for the explanation of the macOS EOF trick.
    #[inline]
    fn are_more_entries_remaining(&mut self) -> bool {
        if self.is_end_of_stream() {
            return false;
        }

        const { assert!(Self::BUFFER_SIZE.is_multiple_of(8), "proving alignment") };

        #[cfg(has_eof_trick)]
        // SAFETY: the buffer is 8-aligned and BUFFER_SIZE >= 4, so the last four bytes are in bounds and aligned.
        let last_four_bytes = unsafe {
            NonNull::from_mut(&mut self.syscall_buffer)
                .byte_add(const { Self::BUFFER_SIZE - 4 })
                .cast::<u32>()
        };

        #[cfg(has_eof_trick)]
        // SAFETY: guaranteed in bounds + aligned, zeroed so the kernel's EOF flag is detectable.
        unsafe {
            last_four_bytes.write(0)
        };

        let remaining_bytes = self.getdents();
        let is_more_remaining = remaining_bytes.is_positive();

        #[cfg(has_eof_trick)]
        {
            const { assert!(Self::BUFFER_SIZE >= 1024, "Buffer must be >=1024 for trick") };
            let eof_marker_is_valid: bool = is_more_remaining
                && remaining_bytes.cast_unsigned() <= const { Self::BUFFER_SIZE - 4 };

            self.end_of_stream = (eof_marker_is_valid
                // SAFETY: the marker was initialised before the syscall and the payload didn't overlap it.
                && unsafe { last_four_bytes.read() } & 1 != 0)
                || !is_more_remaining;
        }

        #[cfg(not(has_eof_trick))]
        {
            self.end_of_stream = !is_more_remaining
        }

        self.remaining_bytes = remaining_bytes.max(0).cast_unsigned(); // negatives map to 0
        self.offset = 0;

        is_more_remaining
    }

    /// Gets the next pointer from the `getdents` buffer, refilling it when exhausted.
    ///
    /// Returns ALL entries including `.` and `..`, like `readdir`.
    /// The returned pointer is only valid until the next call.
    #[inline]
    pub fn next_pointer(&mut self) -> Option<Unique<dirent64>> {
        while self.offset >= self.remaining_bytes {
            if !self.are_more_entries_remaining() {
                return None;
            }
        }

        // SAFETY: the buffer holds unread bytes, the offset is in bounds, and the buffer is aligned.
        let drnt = unsafe {
            Unique::from_ref(&self.syscall_buffer)
                .byte_add(self.offset)
                .cast::<dirent64>()
        };
        debug_assert!(drnt.is_aligned(), "Expected to be aligned, report this");

        // SAFETY: we just obtained this pointer, so reading d_reclen is valid.
        let reclen = unsafe { drnt.d_reclen() };
        #[cfg(any(target_os = "linux", target_os = "android"))]
        debug_assert!(reclen >= 24, "expected reclen >=24, it is `{reclen}`");
        self.offset += reclen;

        Some(drnt)
    }
}
