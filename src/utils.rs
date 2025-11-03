#[cfg(not(target_os = "linux"))]
use libc::dirent as dirent64;
#[cfg(target_os = "linux")]
use libc::dirent64;

use crate::memchr_derivations::memrchr;
use core::ops::Deref;

#[inline]
#[cfg(target_os = "linux")]
/*
  Wrapper for direct getdents syscalls


 # Arguments
 - `fd`: Open directory file descriptor
 - `buffer_ptr`: Raw pointer to output buffer
 - `buffer_size`: Size of output buffer in bytes

 # Safety
 - Requires valid open directory descriptor
 - Buffer must be valid for writes of `buffer_size` bytes
 - No type checking on generic pointer(T  must be i8/u8)

 # Returns
 - Positive: Number of bytes read
 - 0: End of directory
 - Negative: Error code (check errno)
*/
pub unsafe fn getdents<T>(fd: i32, buffer_ptr: *mut T, buffer_size: usize) -> libc::c_long
where
    T: crate::ValueType, //i8/u8
{
    // SAFETY:Syscall has no other implicit safety requirements beyond pointer validity
    unsafe { libc::syscall(libc::SYS_getdents64, fd, buffer_ptr, buffer_size) }
}

#[cfg(target_os = "macos")]
#[inline]
/**
  Wrapper for direct getdirentries64 syscalls

 # Arguments
 - `fd`: Open directory file descriptor
 - `buffer_ptr`: Raw pointer to output buffer
 - `nbytes`: Size of output buffer in bytes
 - `basep`: Pointer to location where telldir position is stored

 # Safety
 - Requires valid open directory descriptor
 - Buffer must be valid for writes of `nbytes` bytes
 - No type checking on generic pointer (T must be i8/u8)
 - basep must point to valid memory for `libc::off_t`

 # Returns
 - Positive: Number of bytes read
 - 0: End of directory
 - Negative: Error code (check errno)
*/
pub unsafe fn getdirentries64<T>(
    fd: libc::c_int,
    buffer_ptr: *mut T,
    nbytes: libc::size_t,
    basep: *mut libc::off_t,
) -> i32
where
    T: crate::ValueType,
{
    const SYS_GETDIRENTRIES64: libc::c_int = 344; // Reverse engineered syscall number
    //https://phrack.org/issues/66/16
    // SAFETY: Syscall has no other implicit safety requirements beyond pointer validity
    unsafe { libc::syscall(SYS_GETDIRENTRIES64, fd, buffer_ptr, nbytes, basep) }
}

/// A private trait for types that dereference to a byte slice (`[u8]`) representing file paths.
/// Provides efficient path operations, FFI compatibility, and filesystem interactions.
pub trait BytePath<T>
where
    T: Deref<Target = [u8]>,
{
    fn extension(&self) -> Option<&[u8]>;
    /// Checks if file extension matches given bytes (case-insensitive)
    fn matches_extension(&self, ext: &[u8]) -> bool;

    /// Gets index of filename component start
    fn file_name_index(&self) -> usize;
}

impl<T> BytePath<T> for T
where
    T: Deref<Target = [u8]>,
{
    #[inline]
    fn extension(&self) -> Option<&[u8]> {
        debug_assert!(self.as_ref() != b"/", "root should never go here");
        debug_assert!(!self.is_empty(), "should never be empty");
        // SAFETY: self.len() is guaranteed to be at least 1, as we don't expect empty filepaths (avoid UB check)
        memrchr(b'.', unsafe { self.get_unchecked(..self.len() - 1) }) //exclude cases where the . is the final character
            // SAFETY: The `pos` comes from `memrchr` which searches a slice of `self`.
            // The slice `..self.len() - 1` is a subslice of `self`.
            // Therefore, `pos` is a valid index into `self`.
            // `pos + 1` is also guaranteed to be a valid index.
            // We do this to avoid any runtime checks
            .map(|pos| unsafe { self.get_unchecked(pos + 1..) })
    }

    #[inline]
    fn matches_extension(&self, ext: &[u8]) -> bool {
        self.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    /// Get the length of the basename of a path (up to and including the last '/')
    #[inline]
    fn file_name_index(&self) -> usize {
        debug_assert!(!self.is_empty(), "should never be empty");
        memrchr(b'/', self).map_or(1, |pos| pos + 1)
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
    debug_assert!(!drnt.is_null(),"dirent is null in name length calculation");
    #[cfg(any(
        target_os = "linux",
        target_os = "illumos",
        target_os = "solaris",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "android"
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
        target_os = "android"
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
//cargo-asm --lib fdf::utils::dirent_const_time_strlen (put to inline(never) to display)
#[inline]
#[cfg(any(
    target_os = "linux",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "android"
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
pub const unsafe fn dirent_const_time_strlen(drnt: *const dirent64) -> usize {
    debug_assert!(!drnt.is_null(),"dirent is null in name length calculation");


    #[cfg(not(any(
        target_os = "linux",
        target_os = "illumos",
        target_os = "solaris",
        target_os = "android"
    )))]
    // SAFETY: `dirent` must be validated ( it was required to not give an invalid pointer)
    return unsafe { access_dirent!(drnt, d_namlen) }; //trivial operation for macos/bsds 
    #[cfg(any(
        target_os = "linux",
        target_os = "illumos",
        target_os = "solaris",
        target_os = "android"
    ))]
    // On these systems where we need a bit of 'black magic'
    {
        // Offset from the start of the struct to the beginning of d_name.
        const DIRENT_HEADER_START: usize = core::mem::offset_of!(dirent64, d_name);
        // Access the last field and then round up to find the minimum struct size
        const MINIMUM_DIRENT_SIZE: usize = DIRENT_HEADER_START.next_multiple_of(8);
        const_assert!(
            MINIMUM_DIRENT_SIZE == 24,
            "Minimum dirnt struct size should be 24 on these platforms!"
        );
        use crate::memchr_derivations::HI_U64;
        use crate::memchr_derivations::LO_U64;
        use core::num::NonZeroU64;

        //ignore boiler plate above

        /*  Accessing `d_reclen` is safe because the struct is kernel-provided.
        / SAFETY: `dirent` is valid by precondition */
        let reclen = unsafe { (*drnt).d_reclen } as usize;

        /*
          Read the last 8 bytes of the struct as a u64.
        This works because dirents are always 8-byte aligned. (it is guaranteed aligned by the kernel) */

        // SAFETY: We're indexing in bounds within the pointer. Since the reclen is size of the struct in bytes.
        let last_word: u64 = unsafe { *(drnt.cast::<u8>()).add(reclen - 8).cast::<u64>() };

        #[cfg(target_endian = "little")]
        const MASK: u64 = 0x00FF_FFFFu64;
        #[cfg(target_endian = "big")]
        const MASK: u64 = 0xFFFF_FF00_0000_0000u64; // byte order is shifted unintuitively on big endian!

        /* When the record length is 24/`MINIMUM_DIRENT_SIZE`, the kernel may insert nulls before d_name.
        Which will exist on index's 17/18 (or opposite, for big endian...sigh...)
        Mask them out to avoid false detection of a terminator.
        Multiplying by 0 or 1 applies the mask conditionally without branching. */
        let mask: u64 = MASK * ((reclen == MINIMUM_DIRENT_SIZE) as u64);
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

         SAFETY: The u64 can never be all 0's post-SWAR, therefore we can make a niche optimisation
        https://doc.rust-lang.org/beta/std/intrinsics/fn.ctlz_nonzero.html
        (`NonZeroU64` uses this under the hood)
        (using ctlz_nonzero instruction which is superior to ctlz but can't handle all 0 numbers)
        */
        let zero_bit = unsafe {
            NonZeroU64::new_unchecked(candidate_pos.wrapping_sub(LO_U64) & !candidate_pos & HI_U64)
        };

       

        // Find the position then deduct deduct it from 7 (then add 1 to account for the null ) from the position of the null byte pos
        #[cfg(target_endian = "little")]
        let byte_pos = 8 - (zero_bit.trailing_zeros() >> 3) as usize;
        #[cfg(target_endian = "big")]
        let byte_pos = 8 - (zero_bit.leading_zeros() >> 3) as usize;

        //check final calculation
        debug_assert!(
                reclen - DIRENT_HEADER_START - byte_pos
                //SAFETY: debug only.
                    == unsafe{core::ffi::CStr::from_ptr(access_dirent!(drnt, d_name).cast()).count_bytes() },
                "const swar dirent length calculation failed!"); //Luckily this  stdlib function has a const hack to allow this.(to keep the function const)

      

        /*  Final length:
        total record length - header size - null byte position
        */
        reclen - DIRENT_HEADER_START - byte_pos
    }
}

/*
     assembly output:

        movzx eax, word ptr [rdi + 16]
        xor ecx, ecx
        cmp rax, 24
        mov edx, 16777215
        cmovne rdx, rcx
        or rdx, qword ptr [rdi + rax - 8]
        movabs rcx, -72340172838076673
        add rcx, rdx
        andn rcx, rdx, rcx
        movabs rdx, -9187201950435737472
        and rdx, rcx
        tzcnt rcx, rdx
        shr ecx, 3
        add rax, rcx
        add rax, -27
        ret

*/
