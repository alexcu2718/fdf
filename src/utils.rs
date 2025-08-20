use crate::{DirEntryError, Result, buffer::ValueType};
use core::time::Duration;
#[cfg(not(target_os = "linux"))]
use libc::dirent as dirent64;
#[cfg(target_os = "linux")]
use libc::dirent64;
use std::time::{SystemTime, UNIX_EPOCH};
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
/// Via specialised instructions, AVX2 if available, then SSE2 then libc's strlen
///
/// # Returns the number of bytes not including the null terminator.
///
/// # Safety
/// This function is `unsafe` because it dereferences a raw pointer, it will not work on 0 length strings, they MUST be null-terminated.
///
/// Uses AVX2 if compiled with flags otherwise SSE2 if available, failng that, `libc::strlen`.
/// Interesting benchmarks resuls:
/// It's faster than my constant time strlen for dirents for small strings, but after 32 bytes, it becomes slower.
/// It is also faster than the libc implementation but only for size below 128...?wtf.
#[inline]
#[allow(clippy::undocumented_unsafe_blocks)] //not commenting all of this.
#[allow(clippy::multiple_unsafe_ops_per_block)] //DIS
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
            use core::arch::x86_64::{
                __m256i,
                _mm256_cmpeq_epi8,    // Compare 32 bytes at once
                _mm256_loadu_si256,   // Unaligned 32-byte load
                _mm256_movemask_epi8, // Bitmask of null matches
                _mm256_setzero_si256, // Zero vector
            };

            let mut offset = 0;
            loop {
                let chunk = _mm256_loadu_si256(ptr.add(offset).cast::<__m256i>()); //load the pointer, add offset, cast to simd type
                let zeros = _mm256_setzero_si256(); //zeroise vector
                let cmp = _mm256_cmpeq_epi8(chunk, zeros); //compare each byte in the chunk to 0, 32 at a time,
                let mask = _mm256_movemask_epi8(cmp); //

                if mask != 0 {
                    //find the
                    break offset + mask.trailing_zeros() as usize;
                }
                offset += 32; // Process next 32-byte chunk
            }
        }

        #[cfg(not(target_feature = "avx2"))]
        use core::arch::x86_64::{
            __m128i,
            _mm_cmpeq_epi8,    // Compare 16 bytes
            _mm_loadu_si128,   // Unaligned 16-byte load
            _mm_movemask_epi8, // Bitmask of null matches
            _mm_setzero_si128, // Zero vector
        };
        unsafe {
            let mut offset = 0;
            loop {
                let chunk = _mm_loadu_si128(ptr.add(offset).cast::<__m128i>()); //same as above but for diff instructions
                let zeros = _mm_setzero_si128();
                let cmp = _mm_cmpeq_epi8(chunk, zeros);
                let mask = _mm_movemask_epi8(cmp);

                if mask != 0 {
                    //U
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
        unsafe { libc::strlen(ptr.cast::<_>()) } //not we inventing the wheel
    }
}

#[inline]
///  constructs a path convenience (just a utility function to save verbosity)
/// this internal function relies on the pointer to the `dirent64` being non-null
pub unsafe fn construct_path(
    path_buffer: &mut crate::PathBuffer,
    base_len: usize,
    drnt: *const dirent64,
) -> &[u8] {
    // SAFETY: The `drnt` must not be null (checked before using)
    let d_name = unsafe { crate::access_dirent!(drnt, d_name) };
    // SAFETY: as above
    let name_len = unsafe { dirent_name_length(drnt) };
    // SAFETY: The `base_len` is guaranteed to be a valid index into `path_buffer`
    // by the caller of this function.
    let buffer = unsafe { &mut path_buffer.get_unchecked_mut(base_len..) }; //we know base_len is in bounds 
    // SAFETY: `d_name` and `buffer` are known not to overlap because `d_name` is
    // from a `dirent64` pointer and `buffer` is a slice of `path_buffer`.
    // The pointers are properly aligned as they point to bytes. The `name_len`
    // is guaranteed to be within the bounds of `buffer` because the total path
    // length (`base_len + name_len`) is always less than or equal to `LOCAL_PATH_MAX`,
    // which is the capacity of `path_buffer`.
    unsafe { core::ptr::copy_nonoverlapping(d_name, buffer.as_mut_ptr(), name_len) }; //we know these don't overlap and they're properly aligned
    //  SAFETY: The total length `base_len + name_len` is guaranteed to be
    // less than or equal to `LOCAL_PATH_MAX`, which is 4096 or 1024 by default
    unsafe { path_buffer.get_unchecked(..base_len + name_len) }
}

#[inline]
#[allow(clippy::missing_const_for_fn)]
#[allow(clippy::single_call_fn)]
///a utility function for breaking down the config spaghetti that is platform specific optimisations
// i wanted to make this const and separate the function
// because only strlen isn't constant here :(
pub unsafe fn dirent_name_length(drnt: *const dirent64) -> usize {
    #[cfg(target_os = "linux")]
    {
        use crate::dirent_const_time_strlen;
        // SAFETY: `dirent` must be checked before hand to not be null
        unsafe { dirent_const_time_strlen(drnt) } //const time strlen for linux (specialisation)
    }

    #[cfg(any(
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        target_os = "macos"
    ))]
    {
        // SAFETY: `dirent` must be checked before hand to not be null
        unsafe { access_dirent!(drnt, d_namlen) } //specialisation for BSD and macOS, where d_namlen is available
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        target_os = "macos"
    )))]
    {
        // SAFETY: `dirent` must be checked before hand to not be null
        unsafe { libc::strlen(access_dirent!(drnt, d_name).cast::<_>()) }
        // Fallback for other OSes
    }
}

/*
Const-time `strlen` for `dirent64::d_name` using SWAR bit tricks.
/// (c) [Alexander Curtis .
/// My Cat Diavolo is cute.
/// */

#[inline]
#[cfg(target_os = "linux")]
#[allow(clippy::multiple_unsafe_ops_per_block)]
#[allow(clippy::cast_ptr_alignment)] //we're aligned (compiler can't see it though and we're doing fancy operations)
/// Const-fn strlen for dirent's `d_name` field using bit tricks, no SIMD.
/// Constant time (therefore branchless)
///
/// This function can't really be used in a const manner, I just took the win where I could! ( I thought it was cool too...)
/// It's probably the most efficient way to calculate the length
/// It calculates the length of the `d_name` field in a `libc::dirent64` structure without branching on the presence of null(kernel guaranteed)
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
/// - so we only need to scan at most 255 bytes. However, since we read the last 8 bytes and apply bit tricks,
/// - we can locate the null terminator with a single 64-bit read and mask
///                    
///
///
/// # SAFETY
/// The caller must uphold the following invariants:
/// - The `dirent` pointer must point to a valid `libc::dirent64` structure
pub const unsafe fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
    use crate::memchr_derivations::find_zero_byte_u64;
    const DIRENT_HEADER_START: usize = core::mem::offset_of!(libc::dirent64, d_name) + 1; //we're going backwards(to the start of d_name) so we add 1 to the offset
    // SAFETY: `dirent` must be validly checked before passing to this function
    // It points to a properly initialised `dirent` struct,
    // so reading the `d_reclen` field is safe.
    let reclen = unsafe { (*dirent).d_reclen } as usize; //(do not access it via byte_offset!)
    debug_assert!(
        reclen.is_multiple_of(8),
        "reclen should be a multiple of 8!"
    ); //show it's always aligned 
    // Calculate find the  start of the d_name field
    //  Access the last 8 bytes(word) of the dirent structure as a u64
    //because unaligned reads are expensive
    #[cfg(target_endian = "little")]
    // SAFETY: `dirent` is a valid pointer to a struct whose size is always a multiple of 8.
    // `reclen - 8` therefore points to the last properly aligned 8-byte word in the struct,
    let last_word:u64 = unsafe { *(dirent.cast::<u8>()).add(reclen - 8).cast::<u64>() }; //go to the last word in the struct.
    // SAFETY: The dirent struct is always a multiple of 8
    #[cfg(target_endian = "big")]
    let last_word:u64 = unsafe { *(dirent.cast::<u8>()).add(reclen - 8).cast::<u64>() }.to_le();
    //TODO! this could probably be optimised, testing anything on bigendian is a fucking pain because it takes an ungodly time to compile.
    // Special case: When processing the 3rd u64 word (index 2), we need to mask
    // the non-name bytes (d_type and padding) to avoid false null detection.
    //  Access the last 8 bytes(word) of the dirent structure as a u64 word
    // The 0x00FF_FFFF mask preserves only the 3 bytes where the name could start.
    // Branchless masking: avoids branching by using a mask that is either 0 or 0x00FF_FFFF
    // Special handling for 24-byte records (common case):
    // Mask out non-name bytes (d_type and padding) that could cause false null detection
    // When the d_name is  4 bytes or fewer, the kernel places null bytes at the start of the d_name, we need to mask them out 
    let mask:u64 = 0x00FF_FFFFu64 * ((reclen == 24) as u64); // (multiply by 0 or 1)
    // The mask is applied to the last word to isolate the relevant bytes.
    // The last word is masked to isolate the relevant bytes,
    //we're bit manipulating the last word (a byte/u64) to find the first null byte
    //this boils to a complexity of strlen over 8 bytes, which we then accomplish with a bit trick
    // Combine the word with our mask to ensure:
    // - Original name bytes remain unchanged
    // - Non-name bytes are set to 0xFF (guaranteed non-zero)
    let candidate_pos:u64 = last_word | mask;
    // The resulting value (`candidate_pos`) has:
    // - Original name bytes preserved
    // - Non-name bytes forced to 0xFF (guaranteed non-zero)
    // - Maintains the exact position of any null bytes in the name
    //I have changed the definition since the original README, I found a more rigorous backing!
    // We subtract 7 to get the correct offset in the d_name field.
    let byte_pos = 7 - find_zero_byte_u64(candidate_pos); // a constant time SWAR function
    // The final length is calculated as:
    // `reclen - DIRENT_HEADER_START - byte_pos`
    // This gives us the length of the d_name field, excluding the header and the null
    // byte position.
    reclen - DIRENT_HEADER_START - byte_pos
}
