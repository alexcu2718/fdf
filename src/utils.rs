use crate::dirent64;
use crate::memchr_derivations::memrchr;
use core::ops::Deref;

#[inline]
#[cfg(any(target_os = "linux", target_os = "android"))]
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

// #[cfg(target_os = "macos")]
// #[inline]
// /**
//   Wrapper for direct getdirentries64 syscalls

//  # Arguments
//  - `fd`: Open directory file descriptor
//  - `buffer_ptr`: Raw pointer to output buffer
//  - `nbytes`: Size of output buffer in bytes
//  - `basep`: Pointer to location where telldir position is stored

//  # Safety
//  - Requires valid open directory descriptor
//  - Buffer must be valid for writes of `nbytes` bytes
//  - No type checking on generic pointer (T must be i8/u8)
//  - basep must point to valid memory for `libc::off_t`

//  # Returns
//  - Positive: Number of bytes read
//  - 0: End of directory
//  - Negative: Error code (check errno)
// */
// pub unsafe fn getdirentries64<T>(
//     fd: libc::c_int,
//     buffer_ptr: *mut T,
//     nbytes: libc::size_t,
//     basep: *mut libc::off_t,
// ) -> i32
// where
//     T: crate::ValueType,
// {
//     const SYS_GETDIRENTRIES64: libc::c_int = 344; // Reverse engineered syscall number
//     //https://phrack.org/issues/66/16
//     // We verify this works via build script, we check if `getdirentries` returns >0 for tmp directory, if not, syscall is broken.
//     // SAFET******: Syscall has no other implicit safety requirements beyond pointer validity
//     unsafe { libc::syscall(SYS_GETDIRENTRIES64, fd, buffer_ptr, nbytes, basep) }
// }

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
        debug_assert!(!self.is_empty(), "should never be empty");
        // SAFETY: self.len() is guaranteed to be at least 1, as we don't expect empty filepaths (avoid UB check)
        memrchr(b'.', unsafe {
            self.get_unchecked(..self.len().saturating_sub(1))
        }) //exclude cases where the . is the final character
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
    /// Returns 0 for length 1 byte paths
    #[inline]
    fn file_name_index(&self) -> usize {
        if __is_length_one(self) {
            return 0;
        }
        debug_assert!(!self.is_empty(), "should never be empty");
        memrchr(b'/', self).map_or(1, |pos| pos + 1)
    }
}

#[cold]
const fn __is_length_one(bytes: &[u8]) -> bool {
    bytes.len() == 1
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
    debug_assert!(!drnt.is_null(), "dirent is null in name length calculation");
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "emscripten",
        target_os = "illumos",
        target_os = "solaris",
        target_os = "redox",
        target_os = "hermit",
        target_os = "fuchsia",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "aix",
        target_os = "hurd"
    ))]
    {
        // SAFETY: `dirent` must be checked before hand to not be null
        unsafe { dirent_const_time_strlen(drnt) } //const time strlen for the above platforms (specialisation)
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "emscripten",
        target_os = "illumos",
        target_os = "solaris",
        target_os = "redox",
        target_os = "hermit",
        target_os = "fuchsia",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "aix",
        target_os = "hurd"
    )))]
    {
        // SAFETY: `dirent` must be checked before hand to not be null
        unsafe { libc::strlen(access_dirent!(drnt, d_name)) }
        // Fallback for other OSes, strlen is either on i8 or u8, casting is 0 cast (it's essentially just reinterpreting)
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
    target_os = "android",
    target_os = "emscripten",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "redox",
    target_os = "hermit",
    target_os = "fuchsia",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "aix",
    target_os = "hurd"
))]
#[allow(
    clippy::as_conversions,
    clippy::multiple_unsafe_ops_per_block,
    clippy::host_endian_bytes,
    clippy::cast_ptr_alignment
)] //we're aligned (compiler can't see it though and we're doing fancy operations)
#[must_use]
/**
 Returns the length of a `dirent64's d_name` string in constant time using
 SWAR (SIMD within a register) bit tricks (equivalent to `libc::strlen`, does NOT include the null terminator)

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

# Example
```

#[cfg(any(target_os = "linux", target_os = "android"))]
use libc::dirent64;

#[cfg(not(any(target_os = "linux", target_os = "android")))]
use libc::dirent as dirent64;


use std::env::temp_dir;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use fdf::dirent_const_time_strlen;

let tmp = temp_dir();
let target_path = tmp.join("dirent_const_time_test");
fs::create_dir_all(&target_path).ok();

// Create a test file
let test_file = target_path.join("test_file.txt");
fs::File::create(&test_file).ok();

// Open directory and read entries
let path_cstr = std::ffi::CString::new(target_path.as_os_str().as_bytes()).unwrap();
let dir_fd = unsafe { libc::opendir(path_cstr.as_ptr()) };
if !dir_fd.is_null() {
    let mut entry = unsafe { libc::readdir(dir_fd) };
    while !entry.is_null() {
        let name_len = unsafe {
            dirent_const_time_strlen(entry as *const dirent64)
        };

        let actual_len = unsafe {
            libc::strlen((&raw const (*entry).d_name).cast())
        };
        assert_eq!(name_len, actual_len, "Const-time strlen matches libc strlen");
        entry = unsafe { libc::readdir(dir_fd) };
    }
    unsafe { libc::closedir(dir_fd) };
}

// Cleanup
fs::remove_dir_all(&target_path).ok();
```

 # References
 - [Stanford Bit Twiddling Hacks find 0 byte ](http://www.icodeguru.com/Embedded/Hacker%27s-Delight/043.htm)
 - [find crate `dirent.rs`](https://github.com/Soveu/find/blob/master/src/dirent.rs)

*/
pub const unsafe fn dirent_const_time_strlen(drnt: *const dirent64) -> usize {
    debug_assert!(!drnt.is_null(), "dirent is null in name length calculation");

    #[cfg(any(
        target_os = "macos",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "aix",
        target_os = "hurd"
    ))]
    // SAFETY: `dirent` must be validated ( it was required to not give an invalid pointer)
    return unsafe { (*drnt).d_namlen as usize }; //trivial operation for systems with d_namlen field
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "emscripten", // best effort, no guarantees
        target_os = "illumos",
        target_os = "solaris",
        target_os = "redox", // best effort, no guarantees
        target_os = "hermit", // best effort, no guarantees
        target_os = "fuchsia" // best effort, no guarantees
    ))]
    // On these systems where we need a bit of 'black magic' (no d_namlen field)
    {
        use core::num::NonZeroU64;
        // Offset from the start of the struct to the beginning of d_name.
        const DIRENT_HEADER_START: usize = core::mem::offset_of!(dirent64, d_name);
        // Access the last field and then round up to find the minimum struct size
        const MIN_DIRENT_SIZE: usize = DIRENT_HEADER_START.next_multiple_of(8);
        // A custom macro similar to static_assert from C++ (no runtime cost, crash at compile time!)
        const_assert!(MIN_DIRENT_SIZE == 24, "dirent min size must be 24!");

        const LO_U64: u64 = u64::from_ne_bytes([0x01; size_of::<u64>()]);
        const HI_U64: u64 = u64::from_ne_bytes([0x80; size_of::<u64>()]);

        /*  SAFETY: `dirent` is valid by precondition */
        let reclen = unsafe { (*drnt).d_reclen } as usize;

        /*
          Read the last 8 bytes of the struct as a u64.
        This works because dirents are always 8-byte aligned. (it is guaranteed aligned by the kernel) */

        // SAFETY: We're indexing in bounds within the pointer. Since the reclen is size of the struct in bytes.
        let last_word: u64 = unsafe { *(drnt.byte_add(reclen - 8).cast::<u64>()) };

        // Create a mask for the first 3 bytes in the case where reclen==24, this handles the big endian case too.
        const MASK: u64 = u64::from_ne_bytes([0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00]);

        /* When the record length is 24/`MIN_DIRENT_SIZE`, the kernel may insert nulls before d_name.
        Which will exist on index's 16/17/18 (or opposite, for big endian...sigh...), the d_name starts at 19, so anything before is invalid anyway.

        Mask them out to avoid false detection of a terminator.
        Multiplying by 0 or 1 applies the mask conditionally without branching. */
        let mask: u64 = MASK * ((reclen == MIN_DIRENT_SIZE) as u64);
        /*
         Apply the mask to ignore non-name bytes while preserving name bytes.
         Result:
         - Name bytes remain unchanged
         - Non-name bytes become 0xFF (guaranteed non-zero)
         - Any null terminator in the name remains detectable
        */
        let candidate_pos: u64 = last_word | mask;

        /*
          SWAR null detection algorithm:
         Convert each zero byte to 0x80 and non-zero bytes to 0x00 using bit tricks.
         This allows us to identify the position of the first null terminator in parallel.

         The formula: (candidate - 0x010101...) & ~candidate & 0x808080...
          - candidate - 0x01...: Creates 0xFF in bytes where candidate was 0x00
         - & ~candidate: Ensures we only mark bytes that were originally zero
          - & 0x80...: Isolates the high bit of each byte for null detection

           Check hackers delight reference above for better explanation.

          Then use a niche optimisation, because the last word will ALWAYS contain a null terminator, we can use `NonZeroU64`,
          This has the benefit of using a smarter intrinsic
          https://doc.rust-lang.org/src/core/num/nonzero.rs.html#599
        https://doc.rust-lang.org/beta/std/intrinsics/fn.ctlz_nonzero.html
        https://doc.rust-lang.org/beta/std/intrinsics/fn.cttz_nonzero.html

        This allows us to skip a 0 check which then allows us to use tzcnt on most cpu's

             Check hackers delight reference above for better explanation.
         */

        //SAFETY: The u64 can never be all 0's post-SWAR
        let zero_bit = unsafe {
            NonZeroU64::new_unchecked(candidate_pos.wrapping_sub(LO_U64) & !candidate_pos & HI_U64)
        };

        // Find the position of the null terminator
        #[cfg(target_endian = "little")]
        let byte_pos = (zero_bit.trailing_zeros() >> 3) as usize;
        #[cfg(target_endian = "big")]
        let byte_pos = (zero_bit.leading_zeros() >> 3) as usize;

        //check final calculation
        debug_assert!(
            reclen - DIRENT_HEADER_START +byte_pos -8
                //SAFETY: debug only.
                    == unsafe{core::ffi::CStr::from_ptr(access_dirent!(drnt, d_name)).count_bytes() },
            "const swar dirent length calculation failed!"
        );
        /*
         Final calculation:
         reclen - DIRENT_HEADER_START = total space available for name
        + byte_pos = position of null within the final 8-byte word
        - 8 = adjust because we started counting from the last 8-byte word
        Example: If null is at position 2 in the last word, we only count those 2 bytes
        from that word toward the total string length.
        */
        reclen - DIRENT_HEADER_START + byte_pos - 8
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
