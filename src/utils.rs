#![allow(dead_code)]
use crate::buffer::ValueType;
use crate::{DirEntryError, Result, cstr};

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

/// Uses AVX2 if compiled with flags otherwise SSE2 if available, failng that, libc::strlen.
#[inline]
#[allow(clippy::ptr_as_ptr)] //safe to do this as u8 is aligned to 16 bytes
///Deprecated in favour of a macro (`strlen_asm!`)
// SAFETY: the caller must guarantee that `ptr` points to a valid null-terminated string of type `T` and does not start with a null byte.
pub unsafe fn strlen<T>(ptr: *const T) -> usize
where
    T: ValueType,
{
    unsafe{crate::strlen_asm!(ptr)}
    
}



#[inline]
#[allow(clippy::items_after_statements)]
#[allow(clippy::cast_possible_truncation)] //stupid
#[allow(clippy::inline_asm_x86_intel_syntax)]
#[cfg(target_arch = "x86_64")]
/// Opens a directory using an assembly implementation of open(to reduce libc overplay) and returns the file descriptor.
/// Returns -1 on error.
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    let filename: *const u8 =unsafe{cstr!(bytepath)};
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

#[inline]
#[cfg(not(target_arch="x86_64"))]
/// Opens a directory using libc's open function. Backup function for non-x86_64 architectures.
/// Returns -1 on error.
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {

    unsafe {libc::open(cstr!(bytepath), libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK | libc::O_RDONLY) }
}


#[inline]
#[cfg(not(target_arch="x86_64"))]
pub unsafe fn close_asm(fd: i32) {
    unsafe { libc::close(fd) };
}



#[inline]
#[allow(clippy::inline_asm_x86_intel_syntax)]
#[cfg(target_arch = "x86_64")]
pub unsafe fn close_asm(fd: i32) {
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

//internal convenients functions for min/max
pub(crate) const fn const_min(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}
pub(crate) const fn const_max(a: usize, b: usize) -> usize {
    if a < b { b } else { a }
}

#[inline]
#[allow(clippy::integer_division)] //i know reclen is always a multiple of 8, so this is fine
#[allow(clippy::cast_possible_truncation)] //^
#[allow(clippy::integer_division_remainder_used)] //division is fine.
#[allow(clippy::ptr_as_ptr)] //safe to do this as u8 is aligned to 8 bytes
/// Const-fn strlen for dirent's `d_name` field using bit tricks.
///
/// This function is designed to be used in a constant-time context, I just thought it was cool!
/// It's probably the most efficient way to calculate the length
/// It calculates the length of the `d_name` field in a `libc::dirent64` structure without branching on the presence of null bytes.
/// It needs to be used on  a VALID `libc::dirent64` pointer, and it assumes that the `d_name` field is null-terminated.
/// 
/// This is my own implementation of a constant-time strlen for dirents, which is useful for performance/learning.
/// 
/// Reference <https://github.com/lattera/glibc/blob/master/string/strlen.c#L1>    
///                                   
/// Reference <https://graphics.stanford.edu/~seander/bithacks.html#HasZeroByte>    
///                        
/// Reference <https://github.com/Soveu/find/blob/master/src/dirent.rs>           
///                    
/// Combining all these tricks, i made this beautiful thing!
/// # SAFETY
/// This function is `unsafe` because it performs raw pointer dereferencing and unchecked pointer arithmetic.
/// The caller must uphold the following invariants:
/// - The `dirent` pointer must point to a valid `libc::dirent64` structure
pub const unsafe fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
        const DIRENT_HEADER_SIZE: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1;
        let reclen = unsafe{(*dirent).d_reclen as usize}; // we MUST cast this way, as it is not guaranteed to be aligned, so we can't use offset_ptr!() here
        // Calculate the number of u64 words in the record length
        // Calculate find the  start of the d_name field
          let last_word = unsafe{*((dirent as *const u8).add(reclen - 8) as *const u64)};
        // Special case: When processing the 3rd u64 word (index 2), we need to mask
        // the non-name bytes (d_type and padding) to avoid false null detection.
        // The 0x00FF_FFFF mask preserves only the 3 bytes where the name could start.
        // Branchless masking: avoids branching by using a mask that is either 0 or 0x00FF_FFFF
          #[allow(clippy::cast_lossless)] //shutup
        // Branchless 3rd-word mask (0x00FF_FFFF if index==2 else 0)
        let mask = 0x00FF_FFFFu64 * ((reclen / 8 == 3) as u64);// (multiply by 0 or 1)
        //we're bit manipulating the last word (a byte/u64) to find the first null byte
        //this boils to a complexity of strlen over 8 bytes, which we then accomplish with a bit trick
        // The mask is applied to the last word to isolate the relevant bytes.
        // The last word is masked to isolate the relevant bytes, and then we find the first zero byte.
        // the kernel guarantees that the d_name field is null-terminated, so we can safely use this trick.
        let zero_bit = (last_word | mask).wrapping_sub(0x0101_0101_0101_0101)
           & !(last_word | mask)
            & 0x8080_8080_8080_8080;


        reclen - DIRENT_HEADER_SIZE - (7 - (zero_bit.trailing_zeros() >> 3) as usize)
        
}
