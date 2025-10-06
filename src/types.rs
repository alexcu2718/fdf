use crate::{AlignedBuffer, DirEntry, DirEntryError, SearchConfig, const_from_env};

///Generic result type for directory entry operations
pub type Result<T> = core::result::Result<T, DirEntryError>;

const_from_env!(
    /// The maximum length of a local path, set to 4096/1024 (Linux/Non-Linux respectively) by default, but can be customised via environment variable.
    LOCAL_PATH_MAX: usize = "LOCAL_PATH_MAX", libc::PATH_MAX
); //set to PATH_MAX, but allow trivial customisation!

//4115==pub const BUFFER_SIZE_LOCAL: usize = crate::offset_of!(libc::dirent64, d_name) + libc::PATH_MAX as usize; //my experiments tend to prefer this. maybe entirely anecdata.
const_from_env!(
    /// The size of the buffer used for directory entries, set to 4115 by default, but can be customised via environment variable.
    /// Meant to be above the size of a page basically
    BUFFER_SIZE:usize="BUFFER_SIZE",std::mem::offset_of!(libc::dirent, d_name) + libc::PATH_MAX as usize
);
//basically this is the should allow getdents to grab a lot of entries in one go

pub type PathBuffer = AlignedBuffer<u8, LOCAL_PATH_MAX>;
#[cfg(target_os = "linux")] //we only use a buffer for syscalls on linux because of stable ABI
pub type SyscallBuffer = AlignedBuffer<u8, BUFFER_SIZE>;

const _: () = assert!(
    LOCAL_PATH_MAX >= libc::PATH_MAX as usize,
    "LOCAL_PATH_MAX too small!"
);

#[cfg(target_os = "linux")] // We only care about the buffer on linux
const_from_env!(
    /// Set a custom page, fairly useless but helpful for compile time assertions because we don't want the page size to be greater than the IOBLOCK
    PAGE_SIZE:usize="FDF_PAGE_SIZE",4096
);

#[cfg(target_os = "linux")] // We only care about the buffer on linux
const _: () = assert!(
    BUFFER_SIZE >= PAGE_SIZE,
    "We expect the buffer to always be greater in capacity than the page"
);

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
