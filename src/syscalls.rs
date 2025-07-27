#[inline]
#[allow(clippy::items_after_statements)]
#[allow(clippy::cast_possible_truncation)] //stupid
#[allow(clippy::inline_asm_x86_intel_syntax)]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
// x86_64-specific implementation
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
/// Opens a directory using direct syscall via assembly
///
/// Uses the `open` syscall with flags optimized for directory scanning:
/// - `O_CLOEXEC`: Close on exec (security)
/// - `O_DIRECTORY`: Fail if not a directory (safety)
/// - `O_NONBLOCK`: Non-blocking operations (performance)
/// - `O_RDONLY`: Read-only access
///
/// # Safety
/// - Requires byte path to be a valid directory
///
/// # Returns
/// - File descriptor (positive integer) on success
/// - -1 on error (check errno for details)
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    use std::arch::asm;
    // Create null-terminated C string from byte slice
    let filename: *const u8 = unsafe { cstr!(bytepath) };
    const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK;
    const SYSCALL_NUM: i32 = libc::SYS_open as _;

    let fd: i32;
    unsafe {
        asm!(
            "syscall",
            inout("rax") SYSCALL_NUM => fd,
            // First argument: path pointer (RDI)
            in("rdi") filename,
            in("rsi") FLAGS,
            in("rdx") libc::O_RDONLY,
            out("rcx") _, out("r11") _,
             // Clobbered registers (resetting)
             // Optimisation hints
            options(nostack, preserves_flags)
        )
    };
    fd
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
/// Opens a directory using `openat` syscall via assembly
///
/// ARM64 uses `openat` instead of `open` for better path resolution.
/// Uses `AT_FDCWD` to indicate relative to current working directory.
///
/// Flags:
/// - `O_CLOEXEC`: Close on exec
/// - `O_DIRECTORY`: Ensure it's a directory
/// - `O_NONBLOCK`: Non-blocking I/O
/// - `O_RDONLY`: Read-only access (mode parameter required but ignored)
///
/// # Safety
/// - Requires byte path to be a valid directory
///
/// # Returns
/// File descriptor on success, -1 on error
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    use std::arch::asm;
    let filename: *const u8 = cstr!(bytepath);

    // aarch64 doesn't have open, we need to use openat for this.
    const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK;
    const MODE: i32 = libc::O_RDONLY; // Required even if unused in directory open
    const SYSCALL_OPENAT: i32 = libc::SYS_openat as i32;

    let fd: i32;
    asm!(
        "svc 0",
        in("x0") libc::AT_FDCWD,          // dirfd = AT_FDCWD
        in("x1") filename,          // pathname
        in("x2") FLAGS,             // flags
        in("x3") MODE,              // mode
        in("x8") SYSCALL_OPENAT,    // syscall number
        lateout("x0") fd,           // return value
        options(nostack)
    );
    fd
}

#[inline]
#[cfg(all(
    target_os = "linux",
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
/// Opens a directory using `libc::open`
///
/// # Safety
/// - Requires byte path to be a valid directory
///
/// # Returns
/// File descriptor on success, -1 on error
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    libc::open(
        cstr!(bytepath),
        libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK | libc::O_RDONLY,
    )
}

#[inline]
#[cfg(all(
    target_os = "linux",
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
/// Closes file descriptor using direct syscall
///
/// # Safety
/// - Takes ownership of the file descriptor
/// - Invalidates fd after call (even on error)
/// - No error checking - intentional for performance
pub unsafe fn close_asm(fd: i32) {
    unsafe { libc::close(fd) };
}

#[inline]
#[allow(clippy::inline_asm_x86_intel_syntax)]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
/// Close directory using direct assembly instructions
/// # Safety
/// - Takes ownership of the file descriptor
/// - Invalidates fd after call (even on error)
/// - No error checking - intentional for performance
pub unsafe fn close_asm(fd: i32) {
    use std::arch::asm;
    let _: isize;
    unsafe {
        asm!(
            "syscall",
            inout("rax") libc::SYS_close => _,
            in("rdi") fd,
            out("rcx") _, out("r11") _,//ignore return
            options(nostack, preserves_flags, nomem)
            //no stack operations,no memory access
        )
    };
}

#[inline]
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
/// Closes file descriptor using direct syscall
///
/// Follows ARM64 syscall conventions:
/// - Syscall number in x8
/// - First argument in x0
///
/// # Safety
/// - Takes ownership of the file descriptor
/// - Invalidates fd after call (even on error)
/// - No error checking - intentional for performance
pub unsafe fn close_asm(fd: i32) {
    use std::arch::asm;
    const SYSCALL_CLOSE: i32 = libc::SYS_close as _;

    unsafe {
        asm!(
            "svc 0",
            in("x0") fd,              // File descriptor (first argument)
            in("x8") SYSCALL_CLOSE,   // Syscall number in x8
            lateout("x0") _,           // Ignore return value
            options(nostack, nomem)    // No stack operations or memory access
        );
    }
}

#[inline]
#[allow(clippy::inline_asm_x86_intel_syntax)]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
/// Reads directory entries using `getdents64` syscall (no libc) for `x86_64/aarm64` (failing that, libc)
///
/// # Arguments
/// - `fd`: Open directory file descriptor
/// - `buffer_ptr`: Raw pointer to output buffer
/// - `buffer_size`: Size of output buffer in bytes
///
/// # Safety
/// - Requires valid open directory descriptor
/// - Buffer must be valid for writes of `buffer_size` bytes
/// - No type checking on generic pointer
///
/// # Returns
/// - Positive: Number of bytes read
/// - 0: End of directory
/// - Negative: Error code (check errno)
pub unsafe fn getdents_asm<T>(fd: i32, buffer_ptr: *const T, buffer_size: usize) -> i64 {
    use std::arch::asm;
    let output;
    unsafe {
        asm!(
            "syscall",
            inout("rax") libc::SYS_getdents64  => output,
            in("rdi") fd,
            in("rsi") buffer_ptr,
            in("rdx") buffer_size,
            out("rcx") _,  // syscall clobbers rcx
            out("r11") _,  // syscall clobbers r11
            options(nostack, preserves_flags)
        )
    };

    output
}

#[inline]
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
/// Reads directory entries using `getdents64` syscall (no libc) for aarch64
///
/// # Arguments
/// - `fd`: Open directory file descriptor
/// - `buffer_ptr`: Raw pointer to output buffer
/// - `buffer_size`: Size of output buffer in bytes
///
/// # Safety
/// - Requires valid open directory descriptor
/// - Buffer must be valid for writes of `buffer_size` bytes
/// - No type checking on generic pointer(must be i8/u8)
///
/// # Returns
/// - Positive: Number of bytes read
/// - 0: End of directory
/// - Negative: Error code (check errno)
pub unsafe fn getdents_asm<T>(fd: i32, buffer_ptr: *const T, buffer_size: usize) -> i64 {
    use std::arch::asm;
    let ret: i64;
    unsafe {
        asm!(
            "svc 0", // Supervisor call
            inout("x0") fd as i64 => ret,  // fd argument and return value
            in("x1") buffer_ptr , //self explanatory naming wins!
            in("x2") buffer_size,
            in("x8") libc::SYS_getdents64 as i64,
              // Clobbered registers (scratch)
            lateout("x16") _,
            lateout("x17") _,
              // Critical: 'memory' indicates buffer may be written
            options(nostack, memory)
        );
    }
    ret
}

#[inline]
#[cfg(all(
    target_os = "linux",
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
/// Libc-based fallback for reading directory entries
/// # Arguments
/// - `fd`: Open directory file descriptor
/// - `buffer_ptr`: Raw pointer to output buffer
/// - `buffer_size`: Size of output buffer in bytes
///
/// # Safety
/// - Requires valid open directory descriptor
/// - Buffer must be valid for writes of `buffer_size` bytes
/// - No type checking on generic pointer(T  must be i8/u8)
///
/// # Returns
/// - Positive: Number of bytes read
/// - 0: End of directory
/// - Negative: Error code (check errno)
pub unsafe fn getdents_asm<T>(fd: i32, buffer_ptr: *const T, buffer_size: usize) -> i64 {
    unsafe { libc::syscall(libc::SYS_getdents64, fd, buffer_ptr, buffer_size) }
}
