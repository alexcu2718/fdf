use crate::buffer::ValueType;

use chrono::{DateTime, Utc};
#[cfg(not(target_os = "linux"))]
use libc::dirent as dirent64;
#[cfg(target_os = "linux")]
use libc::dirent64;

#[must_use]
#[inline]
#[expect(
    clippy::cast_sign_loss,
    reason = "We need to cast into the appropriate type for chrono"
)]
/// Converts Unix timestamp metadata from a `stat` structure to a UTC `DateTime`.
///
/// This function extracts the modification time from a `libc::stat` structure and
/// converts it to a `DateTime<Utc>` object. It handles both the seconds and
/// nanoseconds components of the Unix timestamp.
pub const fn modified_unix_time_to_datetime(st: &libc::stat) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(access_stat!(st, st_mtime), access_stat!(st, st_mtimensec))
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
#[must_use]
#[allow(clippy::missing_const_for_fn)] //strlen isnt const yet the others two are
///a utility function for breaking down the config spaghetti that is platform specific optimisations
// i wanted to make this const and separate the function
// because only strlen isn't constant here :(
unsafe fn dirent_name_length(drnt: *const dirent64) -> usize {
    #[cfg(any(target_os = "linux", target_os = "illumos", target_os = "solaris"))]
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
        target_os = "macos",
        target_os = "illumos",
        target_os = "solaris"
    )))]
    {
        // SAFETY: `dirent` must be checked before hand to not be null
        unsafe { libc::strlen(access_dirent!(drnt, d_name).cast::<_>()) }
        // Fallback for other OSes
    }
}

/*
Const-time `strlen` for `dirent64's d_name` using SWAR bit tricks.
// (c) Alexander Curtis .
// My Cat Diavolo is cute.
//
*/

#[inline]
#[cfg(any(target_os = "linux", target_os = "illumos", target_os = "solaris"))]
#[allow(clippy::multiple_unsafe_ops_per_block)]
#[must_use]
#[allow(clippy::cast_ptr_alignment)] //we're aligned (compiler can't see it though and we're doing fancy operations)
/// Returns the length of a `dirent64's d_name` string in constant time using
/// SWAR (SIMD within a register) bit tricks.
///
/// This function avoids branching and SIMD instructions, achieving O(1) time
/// by reading the final 8 bytes of the structure and applying bit-masking
/// operations to locate the null terminator.
///
/// # Safety
/// The caller must ensure:
/// - `dirent` is a valid, non-null pointer to a `libc::dirent64`.
///
/// # Performance
/// This is one of the hottest paths when scanning directories. By eliminating
/// branches and unnecessary memory reads, it improves efficiency compared with
/// conventional approaches.
///
/// # Implementation Notes
/// - On little-endian platforms the final 8 bytes are read directly.
/// - On big-endian platforms the bytes are converted to little-endian for
///   uniform handling (future improvement possible).
/// - A mask is applied when `reclen == 24` to avoid false positives from
///   padding bytes.
///
/// # References
/// - [Stanford Bit Twiddling Hacks](https://graphics.stanford.edu/~seander/bithacks.html#HasZeroByte)  
/// - [find crate `dirent.rs`](https://github.com/Soveu/find/blob/master/src/dirent.rs)
pub const unsafe fn dirent_const_time_strlen(dirent: *const dirent64) -> usize {
    use crate::memchr_derivations::find_zero_byte_u64;

    // Offset from the start of the struct to the beginning of d_name.
    // We add 1 since we calculate backwards from the header boundary.
    const DIRENT_HEADER_START: usize = core::mem::offset_of!(dirent64, d_name) + 1;

    // Accessing `d_reclen` is safe because the struct is kernel-provided.
    // SAFETY: `dirent` must be a valid pointer to an initialised dirent64 (trivially shown by)
    #[expect(clippy::as_conversions, reason = "Casting u16 to usize is safe")]
    let reclen = unsafe { (*dirent).d_reclen } as usize; // do not use byte_offset here

    debug_assert!(
        reclen.is_multiple_of(8),
        "d_reclen must always be a multiple of 8"
    );

    // Read the last 8 bytes of the struct as a u64.
    // This works because dirents are always 8-byte aligned.
    #[cfg(target_endian = "little")]
    // SAFETY: We're indexing in bounds within the pointer (it is guaranteed aligned by the kernel)
    let last_word: u64 = unsafe { *(dirent.cast::<u8>()).add(reclen - 8).cast::<u64>() };

    // For big-endian targets, convert to little-endian for uniform handling.
    #[cfg(target_endian = "big")]
    // SAFETY: We're indexing in bounds within the pointer (it is guaranteed aligned by the kernel)
    let last_word: u64 = unsafe { *(dirent.cast::<u8>()).add(reclen - 8).cast::<u64>() }.to_le();
    // TODO: big-endian logic could be further optimised. Very much a minor nit.

    // When the record length is 24, the kernel may insert nulls before d_name.
    // Mask them out to avoid false detection of a terminator.
    // Multiplying by 0 or 1 applies the mask conditionally without branching.
    let mask: u64 = 0x00FF_FFFFu64 * ((reclen == 24) as u64);

    // Apply the mask to ignore non-name bytes while preserving name bytes.
    // Result:
    // - Name bytes remain unchanged
    // - Non-name bytes become 0xFF (guaranteed non-zero)
    // - Any null terminator in the name remains detectable
    let candidate_pos: u64 = last_word | mask;

    // Locate the first null byte in constant time using SWAR.
    // Subtract 7 to compute its position relative to the start of d_name.
    let byte_pos = 7 - find_zero_byte_u64(candidate_pos);

    // Final length:
    // total record length - header size - null byte position
    reclen - DIRENT_HEADER_START - byte_pos
}

/*
copypasted from
https://linux.die.net/man/2/getdents64



struct linux_dirent {
    unsigned long  d_ino;     /* Inode number */
    unsigned long  d_off;     /* Offset to next linux_dirent */
    unsigned short d_reclen;  /* Length of this linux_dirent */
    char           d_name[];  /* Filename (null-terminated) */
                        /* length is actually (d_reclen - 2 -
                           offsetof(struct linux_dirent, d_name) */  <-------this is the most important bit, we just need to be careful about padding!
    /*
    char           pad;       // Zero padding byte
    char           d_type;    // File type (only since Linux 2.6.4;
                              // offset is (d_reclen - 1))
    */

}

*/
