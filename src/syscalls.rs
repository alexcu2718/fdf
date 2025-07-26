#[inline]
#[allow(clippy::items_after_statements)]
#[allow(clippy::cast_possible_truncation)] //stupid
#[allow(clippy::inline_asm_x86_intel_syntax)]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
/// Opens a directory using an assembly implementation of open(i'm probably going to learn some bindgen and have some experiments) and returns the file descriptor.
/// Returns -1 on error.
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    use std::arch::asm;
    let filename:*const u8 = unsafe { cstr!(bytepath) };
    const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK;
    const SYSCALL_NUM: i32 = libc::SYS_open as _;

    let fd: i32;
    unsafe {
        asm!(
            "syscall",
            inout("rax") SYSCALL_NUM => fd,
            in("rdi") filename,
            in("rsi") FLAGS,
            in("rdx") libc::O_RDONLY,
            out("rcx") _, out("r11") _,
            options(nostack, preserves_flags)
        )
    };
    fd
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
/// Opens a directory using an assembly implementation of openat and returns the file descriptor.
/// Returns -1 on error.
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    use std::arch::asm;
    let filename: *const u8 = cstr!(bytepath);
    
    // aarch64 doesn't have open, we need to use openat for this.
    const AT_FDCWD: i32 = -100;
    const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK;
    const MODE: i32 = libc::O_RDONLY; // Required even if unused in directory open
    const SYSCALL_OPENAT: i32 = libc::SYS_openat as i32;

    let fd: i32;
    asm!(
        "svc 0",
        in("x0") AT_FDCWD,          // dirfd = AT_FDCWD
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
/// Opens a directory using libc's open function. Backup function for non-x86_64 and non-aarch64 architectures.
/// Returns -1 on error.
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    libc::open(
        cstr!(bytepath),
        libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK | libc::O_RDONLY,
    )
}


#[inline]
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub unsafe fn close_asm(fd: i32) {
    unsafe { libc::close(fd) };
}

#[inline]
#[allow(clippy::inline_asm_x86_intel_syntax)]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub unsafe fn close_asm(fd: i32) {
    use std::arch::asm;
    let _: isize;
    unsafe {
        asm!(
            "syscall",
            inout("rax") libc::SYS_close => _,
            in("rdi") fd,
            out("rcx") _, out("r11") _,
            options(nostack, preserves_flags, nomem)
        )
    };
}