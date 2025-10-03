use crate::buffer::ValueType;

#[cfg(not(target_os = "linux"))]
use libc::dirent as dirent64;
#[cfg(target_os = "linux")]
use libc::dirent64;

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
        unsafe { libc::strlen(ptr.cast::<_>()) } //not reinventing the wheel
    }
}

#[inline]
#[must_use]
#[allow(clippy::missing_const_for_fn)]
/*
strlen isnt const yet the others two are
a utility function for breaking down the config spaghetti that is platform specific optimisations
 i wanted to make this const and separate the function
 because only strlen isn't constant here :(
 */
pub unsafe fn dirent_name_length(drnt: *const dirent64) -> usize {
    #[cfg(any(
        target_os = "linux",
        target_os = "illumos",
        target_os = "solaris",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd"
    ))]
    {
        // SAFETY: `dirent` must be checked before hand to not be null
        unsafe { dirent_const_time_strlen(drnt) } //const time strlen for the above platforms (specialisation)
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "illumos",
        target_os = "solaris",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        target_os = "macos",
    )))]
    {
        // SAFETY: `dirent` must be checked before hand to not be null
        unsafe { libc::strlen(access_dirent!(drnt, d_name).cast::<_>()) }
        // Fallback for other OSes
    }
}

/*
Const-time `strlen` for `dirent64's d_name` using SWAR bit tricks.
 (c) Alexander Curtis .
My Cat Diavolo is cute.

*/

#[inline]
#[cfg(any(
    target_os = "linux",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd"
))]
#[allow(clippy::multiple_unsafe_ops_per_block)]
#[must_use]
#[expect(
    clippy::as_conversions,
    reason = "Casting u16 to usize is only possible const via as casts"
)]
#[allow(clippy::cast_ptr_alignment)] //we're aligned (compiler can't see it though and we're doing fancy operations)
/**
 Returns the length of a `dirent64's d_name` string in constant time using
 SWAR (SIMD within a register) bit tricks.

 This function avoids branching and SIMD instructions, achieving O(1) time
by reading the final 8 bytes of the structure and applying bit-masking
 operations to locate the null terminator.

# Safety
 The caller must ensure:
 `dirent` is a valid, non-null pointer to a `libc::dirent64`.

# Performance
This is one of the hottest paths when scanning directories. By eliminating
 branches and unnecessary memory reads, it improves efficiency compared with
 conventional approaches.


 # References
 - [Stanford Bit Twiddling Hacks](https://graphics.stanford.edu/~seander/bithacks.html#HasZeroByte)
 - [find crate `dirent.rs`](https://github.com/Soveu/find/blob/master/src/dirent.rs)
*/
pub const unsafe fn dirent_const_time_strlen(dirent: *const dirent64) -> usize {
    #[cfg(not(any(target_os = "linux", target_os = "illumos", target_os = "solaris")))]
    // SAFETY: `dirent` must be validated ( it was required to not give an invalid pointer)
    return unsafe { access_dirent!(dirent, d_namlen) }; //trivial operation for macos/bsds 
    #[cfg(any(target_os = "linux", target_os = "illumos", target_os = "solaris"))]
    // Linux/solaris etc type ones where we need a bit of Black magic
    {
        use crate::memchr_derivations::find_zero_byte_u64_optimised;

        // Offset from the start of the struct to the beginning of d_name.
        const DIRENT_HEADER_START: usize = core::mem::offset_of!(dirent64, d_name);

        /*  Accessing `d_reclen` is safe because the struct is kernel-provided.
        / SAFETY: `dirent` is valid by precondition */
        let reclen = unsafe { (*dirent).d_reclen } as usize;

        debug_assert!(
            reclen.is_multiple_of(8),
            "d_reclen must always be a multiple of 8"
        );

        /*
          Read the last 8 bytes of the struct as a u64.
        This works because dirents are always 8-byte aligned. */
        // SAFETY: We're indexing in bounds within the pointer (it is guaranteed aligned by the kernel)
        let last_word: u64 = unsafe { *(dirent.cast::<u8>()).add(reclen - 8).cast::<u64>() };
        /* Note, I don't index as a u64 with eg (reclen-8)/8 because that adds a division which is a costly operations, relatively speaking
        let last_word: u64 = unsafe { *(dirent.cast::<u64>()).add((reclen - 8)/8)}; //this will also work but it's less performant (MINUTELY)
        */

        #[cfg(target_endian = "little")]
        const MASK: u64 = 0x00FF_FFFFu64;
        #[cfg(target_endian = "big")]
        const MASK: u64 = 0xFFFF_FF00_0000_0000u64; // byte order is shifted unintuitively on big endian!

        /* When the record length is 24, the kernel may insert nulls before d_name.
        Which will exist on index's 17/18
        Mask them out to avoid false detection of a terminator.
        Multiplying by 0 or 1 applies the mask conditionally without branching. */
        let mask: u64 = MASK * ((reclen == 24) as u64);
        /*
         Apply the mask to ignore non-name bytes while preserving name bytes.
         Result:
         - Name bytes remain unchanged
         - Non-name bytes become 0xFF (guaranteed non-zero)
         - Any null terminator in the name remains detectable
        */
        let candidate_pos: u64 = last_word | mask;

        /*
         Locate the first null byte in constant time using SWAR.
         Subtract  the position of the index of the 0 then add 1 to compute its position relative to the start of d_name.
         SAFETY: The u64 can never be all 0's post-SWAR, therefore we can make a niche optimisation that won't be made public
        (using ctlz_nonzero instruction which is superior to ctlz but can't handle all 0 numbers)
        */
        let byte_pos = 8 - unsafe { find_zero_byte_u64_optimised(candidate_pos) };
        //let byte_pos=8-find_zero_byte_u64(candidate_pos);

        /*  Final length:
        total record length - header size - null byte position
        */
        reclen - DIRENT_HEADER_START - byte_pos
    }
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
