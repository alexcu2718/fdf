#![allow(dead_code)]
use crate::buffer::ValueType;
use crate::{DirEntryError, Result, cstr};
use libc::dirent64;
#[cfg(target_arch = "x86_64")]
use std::arch::asm;
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
#[allow(clippy::items_after_statements)]
#[allow(clippy::cast_possible_truncation)] //stupid
#[allow(clippy::inline_asm_x86_intel_syntax)]
/// Opens a directory using an assembly implementation of open(to reduce libc overplay) and returns the file descriptor.
/// Returns -1 on error.
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    let filename: *const u8 = cstr!(bytepath); //convert byte slice to C string pointer
    const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK; //construct flags
    const OPEN_SYSCALL: i32 = libc::SYS_open as _; //syscall number for open

    let fd: i32;
    unsafe {
        //typical syscall prelude
        //push r11 and rcx to preserve them, then call syscall
        //after syscall, pop r11 and rcx to restore them
        //this is necessary because the syscall clobbers r11 and rcx, and we need to preserve them.
        asm!("
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
        )
    };
    fd
}
#[inline]
#[allow(clippy::inline_asm_x86_intel_syntax)]
#[allow(clippy::used_underscore_binding)] //its a procedure.
pub unsafe fn close_asm(fd: i32) {
    let _output: isize;
    unsafe {
        asm!("
        push rcx
        push r11
        syscall
        push r11
        popf
        pop r11
        pop rcx",
            inout("rax") libc::SYS_close => _output, //syscall number for close
            in("rdi") fd, // push file descriptor into rdi
            options(preserves_flags, nomem)
        )
    };
}

#[inline]
#[allow(clippy::integer_division_remainder_used)]
#[allow(clippy::ptr_as_ptr)]
#[allow(clippy::integer_division)]
#[allow(clippy::items_after_statements)]
///OK this technically isn't constant time but it's a much lower complexity than the naive approach of iterating over each byte
pub unsafe fn dirent_const_time_strlen(dirent: *const dirent64, reclen: usize) -> usize {
    let reclen_in_u64s = reclen / 8; //reclen is in bytes, we need to convert it to u64s
    // Cast dirent to u64 slice
    // Treat the dirent structure as a slice of u64 for word-wise processing
    //use `std::ptr::slice_from_raw_parts` to create a slice from the raw pointer and avoid ubcheck
    let u64_slice =
        unsafe { &*std::ptr::slice_from_raw_parts(dirent as *const u64, reclen_in_u64s) };
    //  verify alignment/size
    debug_assert!(reclen % 8 == 0 && reclen >= 24, "reclen={reclen}");
    // Calculate position of last word
    // Get the last u64 word in the structure
    let last_word_index = unsafe { reclen_in_u64s.checked_sub(1).unwrap_unchecked() };
    let last_word_check = u64_slice[last_word_index];

    // Special case: When processing the 3rd u64 word (index 2), we need to mask
    // the non-name bytes (d_type and padding) to avoid false null detection.
    // The 0xFFFFFF mask preserves only the LSB 3 bytes where the name could start.
    let last_word_final = if last_word_index == 2 {
        last_word_check | 0x00FF_FFFF //evil integer bit level hacking
    } else {
        //what the fuck?
        last_word_check
    };

    // Find null terminator position within the last word using our repne scasb(very efficient for len<8)
    let ignore = unsafe { 7 - strlen_asm(last_word_final.to_le_bytes().as_ptr()) };

    // Calculate true string length:
    // 1. Skip dirent header (8B d_ino + 8B d_off + 2B reclen + 2B d_type)
    // 2. Subtract ignored bytes (after null terminator in last word)
    const DIRENT_HEADER_SIZE: usize = std::mem::size_of::<u64>()
        + std::mem::size_of::<i64>()
        + std::mem::size_of::<u8>()
        + std::mem::size_of::<u16>()
        + 1; //start pos
    //std::mem::offset_of!(libc::dirent64,d_name)
    //return true strlen
    reclen - DIRENT_HEADER_SIZE - ignore
}
