#![allow(dead_code)]
#[allow(unused_imports)]
use crate::{DirEntryError, Result, buffer::ValueType, cstr, find_zero_byte_u64};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
const DOT_PATTERN: &str = ".";
const START_PREFIX: &str = "/";

/// Convert Unix timestamp (seconds + nanoseconds) to `SystemTime`
/// Not in use currently, later.
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

/// Calculates the length of a null-terminated string pointed to by `ptr`,
/// returning the number of bytes before the null terminator.
///
/// # Safety
/// This function is `unsafe` because it dereferences a raw pointer, it will not work on 0 length strings, they MUST be null-terminated.
///
/// Uses AVX2 if compiled with flags otherwise SSE2 if available, failng that, `libc::strlen`.
/// Interesting benchmarks resuls:
/// It's faster than my constant time strlen for dirents for small strings, but after 32 bytes, it becomes slower.
/// It is also faster than the libc implementation but only for size below 128...?wtf.
#[inline]
#[allow(clippy::unnecessary_safety_comment)] //ill fix this later.
#[allow(unused_unsafe)]
#[allow(clippy::ptr_as_ptr)]
pub unsafe fn strlen<T>(ptr: *const T) -> usize
where
    T: ValueType,
{
    #[cfg(all(
        target_arch = "x86_64",
        any(target_feature = "avx2", target_feature = "sse2")
    ))]
    {
        #[cfg(target_feature = "avx2")]
        unsafe {
            use std::arch::x86_64::{
                __m256i,
                _mm256_cmpeq_epi8,    // Compare 32 bytes at once
                _mm256_loadu_si256,   // Unaligned 32-byte load
                _mm256_movemask_epi8, // Bitmask of null matches
                _mm256_setzero_si256, // Zero vector
            };

            let mut offset = 0;
            loop {
                let chunk = _mm256_loadu_si256(ptr.add(offset) as *const __m256i); //load the pointer, add offset, cast to simd type
                let zeros = _mm256_setzero_si256(); //zeroise vector
                let cmp = _mm256_cmpeq_epi8(chunk, zeros); //compare each byte in the chunk to 0, 32 at a time,
                let mask = _mm256_movemask_epi8(cmp) as i32; //

                if mask != 0 {
                    break offset + mask.trailing_zeros() as usize;
                }
                offset += 32; // Process next 32-byte chunk
            }
        }

        #[cfg(not(target_feature = "avx2"))]
        unsafe {
            use std::arch::x86_64::{
                __m128i,
                _mm_cmpeq_epi8,    // Compare 16 bytes
                _mm_loadu_si128,   // Unaligned 16-byte load
                _mm_movemask_epi8, // Bitmask of null matches
                _mm_setzero_si128, // Zero vector
            };

            let mut offset = 0;
            loop {
                let chunk = _mm_loadu_si128(ptr.add(offset) as *const __m128i);//same as below but for different instructions
                let zeros = _mm_setzero_si128();
                let cmp = _mm_cmpeq_epi8(chunk, zeros);
                let mask = _mm_movemask_epi8(cmp) as i32;

                if mask != 0 {
                    break offset + mask.trailing_zeros() as usize;
                }
                offset += 16; // Process next 16-byte chunk
            }
        }
    }

    #[cfg(not(all(
        target_arch = "x86_64",
        any(target_feature = "avx2", target_feature = "sse2")
    )))]
    {
        unsafe { libc::strlen(ptr.cast::<_>()) }
    }
}

#[inline]
#[allow(clippy::items_after_statements)]
#[allow(clippy::cast_possible_truncation)] //stupid
#[allow(clippy::inline_asm_x86_intel_syntax)]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
/// Opens a directory using an assembly implementation of open(i'm probably going to learn some bindgen and have some experiments) and returns the file descriptor.
/// Returns -1 on error.
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    use std::arch::asm;
    let filename: *const u8 = unsafe { cstr!(bytepath) };
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
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
/// Opens a directory using libc's open function. Backup function for non-x86_64 architectures.
/// Returns -1 on error.
pub unsafe fn open_asm(bytepath: &[u8]) -> i32 {
    unsafe {
        libc::open(
            cstr!(bytepath),
            libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK | libc::O_RDONLY,
        )
    }
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

//internal convenients functions for min/max
pub(crate) const fn const_min(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}
pub(crate) const fn const_max(a: usize, b: usize) -> usize {
    const_min(b, a)
}

/// This function resolves the inode from a `libc::stat` structure in a platform-independent way(well, POSIX way).
/// It is used to get the inode number of a file or directory.
/// It returns a u64 value representing the inode number.
/// The inode number is a unique identifier for a file or directory in the filesystem.
#[inline] //i wanted to skip the boiler plate in other function calls.
pub const fn resolve_inode(libcstat: &libc::stat) -> u64 {
    // This function resolves the inode from a `libc::stat` structure.
    // It is used to get the inode number of a file or directory.
    #[cfg(not(any(
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )))]
    return libcstat.st_ino;
    #[cfg(any(
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    return libcstat.st_ino as u64; // FreeBSD uses u32 for st_ino, so we cast it to u64
}

/*


+-----------------------------------------------------------------------------------------+
| dirent64 STRUCTURE LAYOUT (Little-Endian)                                               |
+--------+--------+--------+--------+--------+--------+--------+--------+-----------------+
| d_ino  | d_off  | reclen | d_type| padding|        d_name[256]                         |
| (8B)   | (8B)   | (2B)   | (1B)  | (1B)   | (variable length, null-terminated)         |
+--------+--------+--------+--------+--------+--------------------------------------------+
                                  ↑         ↑
                                  |         +-- padding byte
                                  +-- d_type byte

+-----------------------------------------------------------------------------------------+
| SWAR ALGORITHM VISUALISATION (last 8 bytes being checked)                               |
+--------+--------+--------+--------+--------+--------+--------+--------+-----------------+
| Byte 0 | Byte 1 | Byte 2 | Byte 3 | Byte 4 | Byte 5 | Byte 6 | Byte 7 |                 |
|        |        |        |        |        |        |        |        |                 |
| 0xNN   | 0xNN   | 0xNN   | 0xNN   | 0xNN   | 0xNN   | 0x00   | 0xNN   | ← null byte found|
+--------+--------+--------+--------+--------+--------+--------+--------+-----------------+
          ↑        ↑        ↑        ↑        ↑        ↑        ↑        ↑
          |        |        |        |        |        |        |        |
          +-- Potential padding/d_type        +-- Actual filename bytes --+

BIT TRICK OPERATION:
1. candidate_pos = last_word | mask
   [0xNN][0xNN][0xNN][0xNN][0xNN][0xNN][0x00][0xNN] ← original
   | OR with mask (0x00FFFFFF when needed)
   ↓
   [0xFF][0xFF][0xNN][0xNN][0xNN][0xNN][0x00][0xNN] ← masked

2. zero_bit calculation:
   (candidate_pos - 0x0101010101010101) & ~candidate_pos & 0x8080808080808080
   ↓
   High bits indicate null bytes: 0x0000000000800000
   ↓
   trailing_zeros() → finds position of first null byte

*//*
Const-time `strlen` for `dirent64::d_name` using SWAR bit tricks.
/// (c) [Alexander Curtis .
/// My Cat Diavolo is cute.
/// */

#[inline]
#[allow(clippy::ptr_as_ptr)] //safe to do this as u8 is aligned to 8 bytes
#[allow(clippy::cast_lossless)] //shutup
/// Const-fn strlen for dirent's `d_name` field using bit tricks, no SIMD.
/// Constant time (therefore branchless)
///
/// This function can't really be used in a const manner, I just took the win where I could! ( I thought it was cool too...)
/// It's probably the most efficient way to calculate the length
/// It calculates the length of the `d_name` field in a `libc::dirent64` structure without branching on the presence of null bytes.
/// It needs to be used on  a VALID `libc::dirent64` pointer, and it assumes that the `d_name` field is null-terminated.
///
/// This is my own implementation of a constant-time strlen for dirents, which is an extremely common operation(probably one of THE hottest functions in this library
/// and ignore/fd etc. So this is a big win!)
///                                   
/// Reference <https://graphics.stanford.edu/~seander/bithacks.html#HasZeroByte>    
///                        
/// Reference <https://github.com/Soveu/find/blob/master/src/dirent.rs>          
///
///
/// Main idea:
/// - We read the last 8 bytes of the `d_name` field, which is guaranteed to be null-terminated by the kernel.
/// This is based on the observation that `d_name` is always null-terminated by the kernel,
///  so we only need to scan at most 255 bytes. However, since we read the last 8 bytes and apply bit tricks,
/// we can locate the null terminator with a single 64-bit read and mask, assuming alignment and endianness.
///                    
/// Combining all these tricks, i made this beautiful thing!
///
/// WORKING ON BIG-ENDIAN AND LITTLE ENDIAN SYSTEMS (linux)
///
/// # SAFETY
/// This function is `unsafe` because...read it
/// The caller must uphold the following invariants:
/// - The `dirent` pointer must point to a valid `libc::dirent64` structure
///  `SWAR` (SIMD Within A Register/bit trick hackery) is used to find the first null byte in the `d_name` field of a `libc::dirent64` structure.
/// THE REASON WE DO THIS IS BECAUSE THE RECLEN IS PADDED UP TO 8 BYTES (rounded up to the nearest multiple of 8),
#[cfg(target_os = "linux")]
pub const unsafe fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1; //we're going backwards(to the start of d_name) so we add 1 to the offset
    let reclen = unsafe { (*dirent).d_reclen } as usize; //(do not access it via byte_offset!)
    //let reclen_new=unsafe{ const {(*dirent).d_reclen}}; //reclen is the length of the dirent structure, including the d_name field
    // Calculate find the  start of the d_name field
    //  Access the last 8 bytes(word) of the dirent structure as a u64 word
    #[cfg(target_endian = "little")]
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }; //DO NOT USE BYTE OFFSET.
    #[cfg(target_endian = "big")]
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }.to_le(); // Convert to little-endian if necessary
    // Special case: When processing the 3rd u64 word (index 2), we need to mask
    // the non-name bytes (d_type and padding) to avoid false null detection.
    //  Access the last 8 bytes(word) of the dirent structure as a u64 word
    // The 0x00FF_FFFF mask preserves only the 3 bytes where the name could start.
    // Branchless masking: avoids branching by using a mask that is either 0 or 0x00FF_FFFF
    // Special handling for 24-byte records (common case):
    // Mask out non-name bytes (d_type and padding) that could cause false null detection
    let mask = 0x00FF_FFFFu64 * ((reclen == 24) as u64); // (multiply by 0 or 1)
    // The mask is applied to the last word to isolate the relevant bytes.
    // The last word is masked to isolate the relevant bytes,
    //we're bit manipulating the last word (a byte/u64) to find the first null byte
    //this boils to a complexity of strlen over 8 bytes, which we then accomplish with a bit trick
    // Combine the word with our mask to ensure:
    // - Original name bytes remain unchanged
    // - Non-name bytes are set to 0xFF (guaranteed non-zero)
    let candidate_pos = last_word | mask;
    // The resulting value (`candidate_pos`) has:
    // - Original name bytes preserved
    // - Non-name bytes forced to 0xFF (guaranteed non-zero)
    // - Maintains the exact position of any null bytes in the name
    //I have changed the definition since the original README, I found a more rigorous backing!
    // We subtract 7 to get the correct offset in the d_name field.
    let byte_pos = 7 - unsafe { find_zero_byte_u64(candidate_pos) }; // a constant time SWAR function
    // The final length is calculated as:
    // `reclen - DIRENT_HEADER_START - byte_pos`
    // This gives us the length of the d_name field, excluding the header and the null
    // byte position.
    reclen - DIRENT_HEADER_START - byte_pos
}
