use crate::{DirEntry, DirEntryError, SearchConfig};

///Generic result type for directory entry operations
pub type Result<T> = core::result::Result<T, DirEntryError>;

//4115==pub const BUFFER_SIZE_LOCAL: usize = crate::offset_of!(libc::dirent64, d_name) + libc::PATH_MAX as usize; //my experiments tend to prefer this. maybe entirely anecdata.
#[cfg(any(target_os = "linux", target_os = "android"))]
const_from_env!(
    /// The size of the buffer used for directory entries, set to 4120 by default, but can be customised via environment variable.
    /// Meant to be above the size of a page basically
    BUFFER_SIZE:usize="BUFFER_SIZE",(std::mem::offset_of!(crate::dirent64, d_name) + libc::PATH_MAX as usize).next_multiple_of(8)
); //TODO investigate this more! 
//basically this is the should allow getdents to grab a lot of entries in one go

#[cfg(any(target_os = "linux", target_os = "android"))]
const_assert!(BUFFER_SIZE >= 4096, "Buffer size too small!");

#[cfg(any(target_os = "linux", target_os = "android"))]
//we only use a buffer for syscalls on linux because of stable ABI(because we don't need to use a buffer for `ReadDir`)
/// A buffer used to  hold the bytes sent from the OS for `getdents` calls
pub type SyscallBuffer = crate::AlignedBuffer<u8, BUFFER_SIZE>;

///filter function type for directory entries,
pub type FilterType = fn(&SearchConfig, &DirEntry, Option<DirEntryFilter>) -> bool;
///generic filter function type for directory entries
pub type DirEntryFilter = fn(&DirEntry) -> bool;

#[derive(Debug)]
/// A safe abstraction around file descriptors for internal IO
pub struct FileDes(pub(crate) i32);

impl FileDes {
    #[must_use]
    #[inline]
    /// Returns a borrowed reference to the underlying file descriptor.
    pub const fn as_borrowed_fd(&self) -> &i32 {
        &self.0
    }

    #[must_use]
    #[inline]
    /// Checks if the file descriptor is currently open
    /// Returns `true` if the file descriptor is open, `false` otherwise
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
