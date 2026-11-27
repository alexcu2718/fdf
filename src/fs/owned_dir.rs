//CURRENTLY UNSURE WHAT TO MAKE PUBLIC ETC.

use crate::{DirEntryError, Unique, dirent64, fs, readdir64, util};
use core::ptr::NonNull;
#[allow(unused)]
use core::{marker::PhantomData, mem::MaybeUninit};

use libc::{DIR, closedir, fdopendir};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, IntoRawFd, OwnedFd, RawFd};

/// A source of raw `dirent64` pointers (`readdir` or a `getdents` buffer) over an open directory fd.
pub trait DirStream: AsFd + AsRawFd + Sized {
    /// Error returned when wrapping an fd (`fdopendir` can fail; `getdents` cannot).
    type Error: Into<DirEntryError>;

    /// Wraps an owned directory fd.
    ///
    /// # Errors
    /// Returns `Self::Error` if the directory stream cannot be created from the fd.
    fn from_owned_fd(fd: OwnedFd) -> Result<Self, Self::Error>;

    /// Returns ALL entries (including `.` and `..`); the pointer is only valid until the next call.
    fn next_pointer(&mut self) -> Option<Unique<dirent64>>;

    /// Returns true if the directory has no entries besides `.` and `..`.
    ///
    /// Doesn't rely on entry sizes or on `.`/`..` being returned first, as the order is unspecified.
    #[inline]
    #[must_use]
    fn is_empty(&mut self) -> bool {
        while let Some(drnt) = self.next_pointer() {
            skip_dot_or_dot_dot_entries!(drnt, continue);
            return false;
        }
        true
    }
}

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
    type Error = DirEntryError;

    /// Wraps an owned file descriptor in a directory stream.
    ///
    /// # Errors
    /// Returns the OS error if `fdopendir` fails (e.g. the fd is not a directory).
    #[inline]
    fn try_from(fd: OwnedFd) -> fs::Result<Self> {
        Self::from_owned_fd(fd)
    }
}

impl DirStream for OwnedDir<'_> {
    type Error = DirEntryError;

    /// Wraps an owned file descriptor in a directory stream.
    ///
    /// # Errors
    /// Returns the OS error if `fdopendir` fails (e.g. the fd is not a directory).
    #[inline]
    fn from_owned_fd(fd: OwnedFd) -> fs::Result<Self> {
        // SAFETY: `fd` is a valid, open file descriptor owned by us.
        let ret: *mut DIR = unsafe { fdopendir(fd.as_raw_fd()) };

        match NonNull::new(ret) {
            Some(dirp) => {
                // Ownership moves to the DIR stream, which closes the fd in `closedir`.
                let raw = fd.into_raw_fd();
                // SAFETY: valid ptr, checking for validity for dirfd.
                debug_assert!(raw == unsafe { libc::dirfd(dirp.as_ptr()) }, "checking");
                // strong drop order.
                // SAFETY: `raw` is open and stays valid until `closedir` in `Drop`.
                let fdes = unsafe { BorrowedFd::borrow_raw(raw) };
                Ok(Self { dirp, fdes })
            }
            None => return_os_error!(),
        }
    }

    #[inline]
    fn next_pointer(&mut self) -> Option<Unique<dirent64>> {
        // SAFETY: passing a valid pointer, no other access is possible to this handle
        Unique::new(unsafe { readdir64(self.as_mut_ptr()) })
    }
}

impl OwnedDir<'_> {
    #[inline]
    #[expect(clippy::needless_pass_by_ref_mut, reason = "interior mutability")]
    const fn as_mut_ptr(&mut self) -> *mut DIR {
        self.dirp.as_ptr()
    }
}

#[cfg(has_faster_readdir)]
/**
   Buffer used to  hold the bytes sent from the OS for `getdents`/`getdirentries` calls,

   Aligned(to 8 bytes) stack allocated buffer of [`MaybeUninit`]
*/
#[repr(align(8))] // only used as writable uninit memory.
struct Aligned8Bytes<const SIZE: usize>([u8; SIZE]);

/// An owned directory file descriptor that buffers batched `getdents`/`getdirentries64` reads,
/// mirroring `readdir` semantics (all entries are returned, including `.` and `..`).
///
/// Default `BUFFER_SIZE` is a value chosen from syscall tracing. (Represents call counts)
///
/// Backs [`crate::fs::GetDents`].
#[cfg(has_faster_readdir)]
#[derive(Debug)]
pub struct OwnedFdDir<'fdes, const BUFFER_SIZE: usize = { fs::BUFFER_SIZE }> {
    fd: OwnedFd,
    syscall_buffer: MaybeUninit<Aligned8Bytes<BUFFER_SIZE>>,
    /// Current read position within `syscall_buffer`
    offset: usize,
    /// Number of valid bytes in `syscall_buffer` from the last syscall
    remaining_bytes: usize,
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    /// The base pointer (seek cookie) for the `getdirentries64` call
    base_pointer: libc::off_t,
    /// Set once the kernel reports no more entries
    end_of_stream: bool,
    /// Ties `self` to the lifetime of the fd's origin, mirroring `OwnedDir<'fd>`.
    _marker: PhantomData<BorrowedFd<'fdes>>,
}

#[cfg(has_faster_readdir)] // can't exist for `OwnedDir`
impl<const BUFFER_SIZE: usize> IntoRawFd for OwnedFdDir<'_, BUFFER_SIZE> {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}

#[cfg(has_faster_readdir)]
impl<const BUFFER_SIZE: usize> AsFd for OwnedFdDir<'_, BUFFER_SIZE> {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

#[cfg(has_faster_readdir)]
impl<const BUFFER_SIZE: usize> AsRawFd for OwnedFdDir<'_, BUFFER_SIZE> {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

#[cfg(has_faster_readdir)]
impl<const BUFFER_SIZE: usize> From<OwnedFd> for OwnedFdDir<'_, BUFFER_SIZE> {
    #[inline]
    fn from(fd: OwnedFd) -> Self {
        Self::new(fd)
    }
}

#[cfg(has_faster_readdir)]
impl<const BUFFER_SIZE: usize> OwnedFdDir<'_, BUFFER_SIZE> {
    #[inline]
    pub(crate) const fn new(fd: OwnedFd) -> Self {
        const { assert!(BUFFER_SIZE.is_multiple_of(8), "Must be multiple of 8") };
        const { assert!(BUFFER_SIZE >= 128, "Expected buffer size to be above 128") }; //arbitrary low point
        const { assert!(BUFFER_SIZE <= i32::MAX as _, "Size must be below i32::MAX") };
        Self {
            fd,
            syscall_buffer: MaybeUninit::uninit(),
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

     On Linux-like targets this dispatches to [`getdents(64)`], while on macOS and
     FreeBSD it calls [`getdirentries64`] and updates the internal base pointer
     required by that API.

     [`getdents`]: https://man7.org/linux/man-pages/man2/getdents.2.html
     [`getdirentries64`]: https://man7.org/linux/man-pages/man3/getdirentries.3.html
    */
    #[inline] //TODO MAKE PUBLIC?
    pub(crate) fn getdents(&mut self) -> isize {
        #[cfg(not(any(target_os = "macos", target_os = "freebsd")))]
        {
            // SAFETY: passing a valid buffer of `BUFFER_SIZE` bytes to an open file descriptor
            unsafe {
                util::getdents64(
                    self.as_raw_fd(),
                    self.syscall_buffer.as_mut_ptr().cast(),
                    BUFFER_SIZE,
                )
            }
        }
        #[cfg(any(target_os = "macos", target_os = "freebsd"))]
        {
            // SAFETY: passing a valid buffer to an open file descriptor and base pointer
            unsafe {
                util::getdirentries64(
                    self.as_raw_fd(),
                    self.syscall_buffer.as_mut_ptr().cast(),
                    BUFFER_SIZE,
                    &raw mut self.base_pointer,
                )
            }
        }
    }

    /// Refills the buffer, returns whether any bytes were read.
    #[inline]
    fn are_more_entries_remaining(&mut self) -> bool {
        if self.is_end_of_stream() {
            return false;
        }

        const { assert!(BUFFER_SIZE.is_multiple_of(8), "proving alignment") };
        const { assert!(BUFFER_SIZE >= 512, "Should be reasonably bigger.") }

        #[cfg(has_eof_trick)]
        // SAFETY: the buffer is 8-aligned and BUFFER_SIZE >= 4, so the last four bytes are in bounds and aligned.
        let last_four_bytes = unsafe {
            let ptr = NonNull::from_mut(&mut self.syscall_buffer)
                .byte_add(const { BUFFER_SIZE - 4 })
                .cast::<u32>();

            ptr.write(0);
            ptr
        };

        let remaining_bytes = self.getdents();
        let is_more_remaining = remaining_bytes.is_positive();

        #[cfg(has_eof_trick)] // See for trick
        {
            // https://github.com/apple-oss-distributions/Libc/blob/899a3b2d52d95d75e05fb286a5e64975ec3de757/gen/FreeBSD/opendir.c#L373-L392
            const { assert!(BUFFER_SIZE >= 1024, "Buffer must be >=1024 for trick") };
            // SAFETY: the marker was initialised before the syscall and the payload didn't overlap it.
            self.end_of_stream = !is_more_remaining
                || (remaining_bytes.cast_unsigned() <= const { BUFFER_SIZE - 4 }
                    && unsafe { last_four_bytes.read() } & 1 != 0);
        }

        #[cfg(not(has_eof_trick))]
        {
            self.end_of_stream = !is_more_remaining
        }

        self.remaining_bytes = remaining_bytes.max(0).cast_unsigned(); // negatives map to 0
        self.offset = 0;

        is_more_remaining
    }
}

#[cfg(has_faster_readdir)]
impl<const BUFFER_SIZE: usize> DirStream for OwnedFdDir<'_, BUFFER_SIZE> {
    type Error = core::convert::Infallible; // needs to match API of readdir sigh.
    // Move to never type eventually.

    #[inline]
    fn from_owned_fd(fd: OwnedFd) -> Result<Self, Self::Error> {
        Ok(Self::new(fd))
    }

    /// Gets the next pointer from the `getdents` buffer, refilling it when exhausted.
    ///
    /// Returns ALL entries including `.` and `..`, like `readdir`.
    /// The returned pointer is only valid until the next call.
    #[inline]
    fn next_pointer(&mut self) -> Option<Unique<dirent64>> {
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
        self.offset += unsafe { drnt.d_reclen() };

        Some(drnt)
    }
}
