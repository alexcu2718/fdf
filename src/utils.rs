#![allow(dead_code)]
#[cfg(target_arch = "x86_64")]
use std::arch::asm;
use crate::buffer::ValueType;
use crate::{DirEntryError, Result,cstr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DOT_PATTERN: &str = ".";
const START_PREFIX: &str = "/";

/// Get the length of the basename of a path (up to and including the last '/')
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn get_baselen(path: &[u8]) -> u16 {
    path.rsplitn(2, |&c| c == b'/')
        .nth(1)
        .map_or(1, |parent| parent.len() + 1) as _ // +1 to include trailing slash etc
}

/// Convert Unix timestamp (seconds + nanoseconds) to `SystemTime`
#[allow(clippy::missing_errors_doc)] //fixing errors later
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
pub fn unix_time_to_system_time(sec: i64, nsec: i32) -> Result<SystemTime> {
    let (base, offset) = if sec >= 0 {
        (UNIX_EPOCH, Duration::new(sec as u64, nsec as u32))
    } else {
        let sec_abs = sec.unsigned_abs();
        (
            UNIX_EPOCH + Duration::new(sec_abs, 0),
            Duration::from_nanos(nsec as u64),
        )
    };

    base.checked_sub(offset)
        .or_else(|| UNIX_EPOCH.checked_sub(Duration::from_secs(0)))
        .ok_or(DirEntryError::TimeError)
}

#[cfg(target_arch = "x86_64")]
#[allow(clippy::inline_asm_x86_intel_syntax)]
#[inline]
/// Uses inline assembly to calculate the length of a null-terminated string.
/// this is specifically more efficient for small strings, which dirent `d_name`'s usually are.
/// This function is only available on `x86_64` architectures.
/// it's generic over the `ValueType`, which is i8 or u8.
pub(crate) unsafe fn strlen_asm<T>(s: *const T) -> usize
where
    T: ValueType,
{
    //aka i8/u8
    unsafe {
        let len: usize;
        core::arch::asm!(
        "mov rdi, {ptr}", //move pointer to rdi
        "xor eax, eax",  // xor is smaller than xor al,al
        "mov rcx, -1",   // directly set to max instead of xor/not
        "repne scasb",
        "not rcx", // invert rcx to get the length
        "dec rcx", //subtract 1 to account for null
        "mov {len}, rcx", //move length to len
            ptr = in(reg) s,
            len = out(reg) len,
            out("rdi") _,  // mark rdi as clobbered
            out("rcx") _,  // mark ^ as clobbered
            out("al") _,   // mark ^ as clobbered
        );

        len
    }
}

#[cfg(not(target_arch = "x86_64"))]
#[inline]
///Uses libc's strlen function to calculate the length of a null-terminated string.
/// it's generic over the `ValueType`, which is i8 or u8.
pub(crate) unsafe fn strlen_asm<T>(s: *const T) -> usize
where
    T: ValueType, //aka i8/u8
{
    unsafe { libc::strlen(s.cast::<i8>()) }
}






 #[inline]
    /// Opens a directory using `libc::opendir` and returns the file descriptor.
    /// Returns -1 on error.
    pub unsafe fn open_asm(bytepath:&[u8]) -> i32 {
        let filename:*const u8=cstr!(bytepath);//convert byte slice to C string pointer
        const FLAGS:i32=libc::O_CLOEXEC  | libc::O_DIRECTORY;// | libc::O_NONBLOCK; //construct flags
        const OPEN_SYSCALL:i32= libc::SYS_open as _; //syscall number for open


       
    let fd:i32;
    unsafe{asm!("
        push rcx
        push r11
        syscall
        push r11
        popf
        pop r11
        pop rcx",
        inout("rax") OPEN_SYSCALL => fd, //syscall number for open
        in("rdi") filename, //load filename into rdi
        in("rsi") FLAGS, //load flags
        in("rdx") libc::O_RDONLY , //mode (0)
        options(preserves_flags,readonly)
    )};
    return fd
}