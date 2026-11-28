//CURRENTLY UNSURE WHAT TO MAKE PUBLIC ETC.
use crate::{DirEntryError, Unique, dirent64, fs, readdir64, util};
#[allow(unused)]
use core::mem::{ManuallyDrop, MaybeUninit};
use core::ptr::NonNull;
use libc::{DIR, closedir, fdopendir};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, IntoRawFd, OwnedFd, RawFd};

/// A source of raw `dirent64` pointers (`readdir` or a `getdents` buffer) over an open directory fd.
pub trait DirStream: AsFd + AsRawFd + Sized {
    /// Wraps an owned directory fd.
    ///
    /// # Errors
    /// Returns an error if the directory stream cannot be created from the fd
    /// (`fdopendir` can fail; the `getdents` implementation never does).
    fn from_owned_fd(fd: OwnedFd) -> fs::Result<Self>;

    /// Returns ALL entries (including `.` and `..`); the pointer is only valid until the next call.
    fn next_pointer(&mut self) -> Option<Unique<dirent64>>;

    /// Returns true if the directory has no entries besides `.` and `..`.
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
pub struct OwnedDir {
    dirp: NonNull<DIR>, // We store this but just need it for deref,we only acccess the pointer from within
    fdes: ManuallyDrop<OwnedFd>, //   We also don't want a non-inlineable glibc call to get the fd. (even if it's just pointer access)
                                 // ManuallyDrop is most appropriate since we *are* manually managing it's drop.
}

impl AsFd for OwnedDir {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fdes.as_fd()
    }
}

impl AsRawFd for OwnedDir {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fdes.as_raw_fd()
    }
}

impl Drop for OwnedDir {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: `dirp` is a valid DIR* from a successful `fdopendir`, closed only here.
        let retval = unsafe { closedir(self.dirp.as_ptr()) };
        debug_assert_eq!(retval, 0, "not closed properly!");
    }
}

impl TryFrom<OwnedFd> for OwnedDir {
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

impl DirStream for OwnedDir {
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
                Ok(Self {
                    dirp,
                    fdes: ManuallyDrop::new(fd),
                })
            }
            None => return_os_error!(),
        }
    }

    #[inline]
    fn next_pointer(&mut self) -> Option<Unique<dirent64>> {
        // SAFETY: passing a valid pointer, no other access is possible to this handle
        Unique::new(unsafe { readdir64(self.dirp.as_ptr()) })
    }
}

#[cfg(has_faster_readdir)] //Buffer used to  hold the bytes sent from the OS for `getdents`/`getdirentries` calls
#[repr(align(8))] // only used as writable uninit memory.
struct AlignedByteBuffer<const SIZE: usize>([u8; SIZE]);

/// An owned directory file descriptor that buffers batched `getdents`/`getdirentries64` reads,
/// mirroring `readdir` semantics (all entries are returned, including `.` and `..`).
///
/// Default `BUFFER_SIZE` is a value chosen from syscall tracing. (Represents call counts)
#[cfg(has_faster_readdir)]
#[derive(Debug)]
pub struct OwnedFdDir<const BUFFER_SIZE: usize = { fs::BUFFER_SIZE }> {
    fd: OwnedFd,
    syscall_buffer: MaybeUninit<AlignedByteBuffer<BUFFER_SIZE>>,
    /// Current read position within `syscall_buffer`
    offset: usize,
    /// Number of valid bytes in `syscall_buffer` from the last syscall
    remaining_bytes: usize,
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    /// The base pointer (seek cookie) for the `getdirentries64` call
    base_pointer: libc::off_t,
    /// Set once the kernel reports no more entries
    is_end_of_stream: bool,
}

#[cfg(has_faster_readdir)] // can't exist for `OwnedDir`
impl<const BUFFER_SIZE: usize> IntoRawFd for OwnedFdDir<BUFFER_SIZE> {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}

#[cfg(has_faster_readdir)]
impl<const BUFFER_SIZE: usize> AsFd for OwnedFdDir<BUFFER_SIZE> {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

#[cfg(has_faster_readdir)]
impl<const BUFFER_SIZE: usize> AsRawFd for OwnedFdDir<BUFFER_SIZE> {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

#[cfg(has_faster_readdir)]
impl<const BUFFER_SIZE: usize> From<OwnedFd> for OwnedFdDir<BUFFER_SIZE> {
    #[inline]
    fn from(fd: OwnedFd) -> Self {
        Self::new(fd)
    }
}

#[cfg(has_faster_readdir)]
impl<const BUFFER_SIZE: usize> OwnedFdDir<BUFFER_SIZE> {
    #[inline]
    pub(crate) const fn new(fd: OwnedFd) -> Self {
        const { assert!(BUFFER_SIZE.is_multiple_of(8), "Must be multiple of 8") };
        const { assert!(BUFFER_SIZE <= i32::MAX as _, "Size must be below i32::MAX") };
        const { assert!(BUFFER_SIZE >= 512, "Should be reasonably bigger.") }
        Self {
            fd,
            syscall_buffer: MaybeUninit::uninit(),
            offset: 0,
            remaining_bytes: 0,
            is_end_of_stream: false,
            #[cfg(any(target_os = "macos", target_os = "freebsd"))]
            base_pointer: 0,
        }
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
        if self.is_end_of_stream {
            return false;
        }
        #[cfg(has_eof_trick)]
        // SAFETY: the buffer is 8-aligned and BUFFER_SIZE >= 4, so the last four bytes are in bounds and aligned.
        let last_four_bytes = unsafe {
            const { assert!(BUFFER_SIZE >= 1024, "Buffer must be >=1024 for trick") }; //See below libj
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
            // SAFETY: the marker was initialised before the syscall and the payload didn't overlap it.
            self.is_end_of_stream = !is_more_remaining
                || (remaining_bytes.cast_unsigned() <= const { BUFFER_SIZE - 4 }
                    && unsafe { last_four_bytes.read() } & 1 != 0);
        }
        #[cfg(not(has_eof_trick))]
        {
            self.is_end_of_stream = !is_more_remaining
        }

        self.remaining_bytes = remaining_bytes.max(0).cast_unsigned(); // negatives map to 0
        self.offset = 0;

        is_more_remaining
    }
}

#[cfg(has_faster_readdir)]
impl<const BUFFER_SIZE: usize> DirStream for OwnedFdDir<BUFFER_SIZE> {
    #[inline]
    fn from_owned_fd(fd: OwnedFd) -> fs::Result<Self> {
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
