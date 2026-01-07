use crate::DirEntryError;
#[cfg(any(target_os = "linux", target_os = "android"))]
use crate::fs::BUFFER_SIZE;

///Generic result type for directory entry operations
pub type Result<T> = core::result::Result<T, DirEntryError>;

/// A buffer used to  hold the bytes sent from the OS for `getdents` calls
/// We only use a buffer for syscalls on linux because of stable ABI(because we don't need to use a buffer for `ReadDir`)
#[cfg(any(target_os = "linux", target_os = "android"))]
pub type SyscallBuffer = crate::fs::AlignedBuffer<u8, BUFFER_SIZE>;

/// A safe abstraction around file descriptors for internal IO
#[derive(Debug)]
#[repr(transparent)]
pub struct FileDes(pub(crate) i32);

impl FileDes {
    /// Returns a borrowed reference to the underlying file descriptor.
    #[must_use]
    #[inline]
    pub const fn as_borrowed_fd(&self) -> &i32 {
        &self.0
    }

    /// Checks if the file descriptor is currently open
    /// Returns `true` if the file descriptor is open, `false` otherwise
    #[must_use]
    #[inline]
    pub fn is_open(&self) -> bool {
        // Use fcntl with F_GETFD to check if the file descriptor is valid
        // If it returns -1 with errno EBADF, the fd is closed
        //SAFETY:  Always safe
        unsafe { libc::fcntl(self.0, libc::F_GETFD) != -1 }
    }

    /**
     Checks if the file descriptor is closed or invalid.
     This is the inverse of [`is_open()`](Self::is_open) and provides
     a more readable alternative for checking closed status.
    */
    #[must_use]
    #[inline]
    pub fn is_closed(&self) -> bool {
        !self.is_open()
    }
}
