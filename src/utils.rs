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

/// Uses SSE2 intrinsics to calculate the length of a null-terminated string.
#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[inline]
#[allow(clippy::ptr_as_ptr)] //safe to do this as u8 is aligned to 16 bytes
///Deprecated in favour of a macro (`strlen_asm!`)
pub unsafe fn strlen_sse2<T>(ptr: *const T) -> usize
where
    T: ValueType,
{
    //aka i8/u8{
    use std::arch::x86_64::{
        __m128i, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_setzero_si128,
    };

    let mut offset = 0;
    loop {
        // Load 16 bytes, alignment is fine because from i8/u8
        let chunk = unsafe { _mm_loadu_si128(ptr.add(offset) as *const __m128i) };

        // Compare against zero byte
        let zeros = unsafe { _mm_setzero_si128() };
        let cmp = unsafe { _mm_cmpeq_epi8(chunk, zeros) };

        // Create a bitmask of results
        let mask = unsafe { _mm_movemask_epi8(cmp) };

        if mask != 0 {
            // we'll get at least one null byte found
            let tz = mask.trailing_zeros() as usize;
            return offset + tz;
        }

        offset += 16;
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
#[cfg(target_arch = "x86_64")]
/// Opens a directory using an assembly implementation of open(to reduce libc overplay) and returns the file descriptor.
/// Returns -1 on error.
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    let filename: *const u8 = cstr!(bytepath);
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
/// Constant-time strlen for dirent's `d_name` field using bit tricks.
///
/// Reference: <https://graphics.stanford.edu/~seander/bithacks.html#HasZeroByte>
/// This function is designed to be used in a constant-time context, I just thought it was cool!
/// It calculates the length of the `d_name` field in a `libc::dirent64` structure without branching on the presence of null bytes.
/// It needs to be used on  a VALID `libc::dirent64` pointer, and it assumes that the `d_name` field is null-terminated.
/// but i modified to make it a const function. I need to benchmark this
/// more comments can be seen in the macro version.
#[allow(clippy::integer_division)] //INTEGER DIVISION IN CONST IS FINE ESPECIALLY.
pub const fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
    //calculate the header+1 beccause we need to get to the start of d_name
    //and then we need to calculate the length of the d_name field
    const DIRENT_HEADER_SIZE: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1;
    let reclen = unsafe { (*dirent).d_reclen as usize };
    let reclen_in_u64s = reclen / 8;
    let last_word_index = reclen_in_u64s - 1;
    let last_word_check =
        unsafe { *(((dirent as *const u8).add(last_word_index * 8)) as *const u64) };
    let is_third_word = (last_word_index == 2) as u64;
    let mask = 0x00FF_FFFFu64 * is_third_word;
    let last_word_final = last_word_check | mask;
    let word_ptr = &last_word_final as *const u64 as *const u8;
    let remainder_len = 7 - {
        let mut i = 0;
        while i < 8 {
            if unsafe { *word_ptr.add(i) } == 0 {
                break;
            }
            i += 1;
        }
        i
    };

    reclen - DIRENT_HEADER_SIZE - remainder_len
}
