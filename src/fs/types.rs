use crate::DirEntryError;

///Generic result type for directory entry operations
pub type Result<T> = core::result::Result<T, DirEntryError>;

/// A buffer used to  hold the bytes sent from the OS for `getdents`/`getdirentries` calls
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "solaris",
    target_os = "illumos"
))]
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

#[cfg(all(any(target_os = "linux", target_os = "android"), not(debug_assertions)))]
pub const BUFFER_SIZE: usize = 8 * 4096;
/*
λ  sudo strace -f fd NOMATCHLOL / -HI 2>&1 | grep getdents | head
[pid 18321] getdents64(3, 0x7ff8e4000cb0 /* 21 entries */, 32768) = 520
[pid 18321] getdents64(3, 0x7ff8e4000cb0 /* 0 entries */, 32768) = 0
[pid 18321] getdents64(3, 0x7ff8e4000cb0 /* 7 entries */, 32768) = 224
[pid 18321] getdents64(3, 0x7ff8e4000cb0 /* 0 entries */, 32768) = 0
[pid 18321] getdents64(3 <unfinished ...>
[pid 18327] getdents64(4 <unfinished ...>




λ  sudo strace -f ls / 2>&1 | grep getdents | head
getdents64(3, 0x557e625c37a0 /* 21 entries */, 32768) = 520
getdents64(3, 0x557e625c37a0 /* 0 entries */, 32768) = 0


*/

#[cfg(all(
    any(target_os = "illumos", target_os = "solaris"),
    not(debug_assertions)
))]
pub const BUFFER_SIZE: usize = 8192;

#[cfg(all(any(target_os = "illumos", target_os = "solaris"), debug_assertions))]
pub const BUFFER_SIZE: usize = 4096;
// Same buffer sizes for illumos/solaris(essentially identical)
/*
alexc@omnios:~% sudo truss -f ls . 2>&1 | grep -Eiv '^/' | grep getdents
6890:      getdents64(3, 0xFEC64000, 8192)                 = 616
6890:   getdents64(3, 0xFEC64000, 8192)                 = 0
alexc@omnios:~%

*/
#[cfg(target_os = "netbsd")]
pub const BUFFER_SIZE: usize = 0x1000;

/*

# kdump | grep getdents | head
 27582  16284 fdfind   CALL  __getdents30(3,0x71a95f64c000,0x1000)
 27582  16284 fdfind   RET   __getdents30 608/0x260
 27582  16284 fdfind   CALL  __getdents30(3,0x71a95f64c000,0x1000)
 27582  16284 fdfind   RET   __getdents30 0
 27582  16284 fdfind   CALL  __getdents30(3,0x71a95f64c000,0x1000)
 27582  16284 fdfind   RET   __getdents30 56/0x38
*/

#[cfg(all(any(target_os = "linux", target_os = "android"), debug_assertions))]
pub const BUFFER_SIZE: usize = 4096; // Crashes during testing due to parallel processes taking up too much stack

#[cfg(target_os = "freebsd")]
pub const BUFFER_SIZE: usize = 4096; // freebsd's buffer size (verified)

#[cfg(all(target_os = "openbsd", not(debug_assertions)))]
pub const BUFFER_SIZE: usize = 0x10000;

#[cfg(all(target_os = "openbsd", debug_assertions))]
pub const BUFFER_SIZE: usize = 4096; //avoid stack overflow during parallelised tests
/*.
foo#  ktrace fd -H . / > /dev/null 2>&1; kdump | grep getdents | head
 57610 fd       CALL  getdents(3,0xad7363e0000,0x10000)
 57610 fd       RET   getdents 688/0x2b0
 57610 fd       CALL  getdents(3,0xad7363e0000,0x10000)
 57610 fd       RET   getdents 0
 57610 fd       CALL  getdents(3,0xad7363e0000,0x10000)
 57610 fd       RET   getdents 2680/0xa78
 57610 fd       CALL  getdents(3,0xad7363e0000,0x10000)
 57610 fd       RET   getdents 0
 57610 fd       CALL  getdents(3,0xad7363e0000,0x10000)
 57610 fd       RET   getdents 856/0x358

*/
#[cfg(all(target_os = "macos", not(debug_assertions)))]
pub const BUFFER_SIZE: usize = 0x2000; //readdir calls this value for buffer size, look at syscall tracing below (8192)

#[cfg(all(target_os = "macos", debug_assertions))]
pub const BUFFER_SIZE: usize = 0x1000; // Give a smaller size to avoid stack overflow when going on tests

/*
/tmp/fdf_test getdirentries ❯ sudo dtruss  fd -HI . 2>&1 | grep getdirentries | head                  ✘ INT alexc@alexcs-iMac 00:52:24


getdirentries64(0x3, 0x7FD166808A00, 0x2000)             = 896 0
getdirentries64(0x3, 0x7FD166808A00, 0x2000)             = 408 0
getdirentries64(0x3, 0x7FD166808A00, 0x2000)             = 288 0


/tmp/fdf_test getdirentries  ❯ sudo dtruss ls . -R 2>&1 | grep getdirentries | head                          alexc@alexcs-iMac 00:58:19

getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 896 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 104 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 1520 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 112 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 344 0

*/
