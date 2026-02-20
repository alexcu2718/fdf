use crate::dirent64;
use crate::util::memchr_derivations::memrchr;
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris"
))]
use core::ffi::c_char;
use core::ops::Deref;

/**
  Wrapper for direct getdents syscalls


 # Arguments
 - `fd`: Open directory file descriptor
 - `buffer_ptr`: Raw pointer to output buffer
 - `buffer_size`: Size of output buffer in bytes

 # Safety
 - Requires valid open directory descriptor
 - Buffer must be valid for writes of `buffer_size` bytes

 # Returns
 - Positive: Number of bytes read
 - 0: End of directory
 - Negative: Error code (check errno)

   This function is only available on Linux/Android/OpenBSD/NetBSD/Illumos/Solaris.
*/
#[inline]
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris"
))]
pub unsafe fn getdents(fd: i32, buffer_ptr: *mut c_char, buffer_size: usize) -> isize {
    #[cfg(any(
        target_os = "openbsd",
        target_os = "solaris",
        target_os = "illumos",
        target_os = "netbsd"
    ))] //Link the function, we can't use the direct syscall because BSD's dont allow it.
    unsafe extern "C" {

        #[cfg_attr(target_os = "netbsd", link_name = "__getdents30")] //special case for NetBSD
        fn getdents(fd: i32, dirp: *mut c_char, count: usize) -> isize;
    }

    // SAFETY: Syscall has no other implicit safety requirements beyond pointer validity(and precursor conditions met.)
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[expect(clippy::cast_possible_truncation, reason = "clong is isize on Unix")]
    unsafe {
        libc::syscall(libc::SYS_getdents64, fd, buffer_ptr, buffer_size) as _
    } // We can do similar linking for getdents64 but prefer not to use the indirection if can be avoided.

    //TODO add dragonfly here(?) TODO once they support Rust 2024
    #[cfg(any(
        target_os = "openbsd",
        target_os = "solaris",
        target_os = "illumos",
        target_os = "netbsd"
    ))]
    // SAFETY: same as above
    unsafe {
        getdents(fd, buffer_ptr, buffer_size)
    }
}

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
#[inline]
/**
  Wrapper for direct getdirentries(64) syscalls

 # Arguments
 - `fd`: Open directory file descriptor
 - `buffer_ptr`: Raw pointer to output buffer
 - `nbytes`: Size of output buffer in bytes
 - `basep`: Pointer to location where telldir position is stored

 # Safety
 - Requires valid open directory descriptor
 - Buffer must be valid for writes of `nbytes` bytes
 - basep must point to valid memory for `libc::off_t`

 # Returns
 - Positive: Number of bytes read
 - 0: End of directory
 - Negative: Error code (check errno)


 This function is only available on macOS/FreeBSD
*/
pub unsafe fn getdirentries64(
    fd: i32,
    buffer_ptr: *mut c_char,
    nbytes: usize,
    basep: *mut i64,
) -> isize {
    use libc::{off_t, size_t, ssize_t};
    // link to libc
    unsafe extern "C" {
        #[cfg_attr(target_os = "macos", link_name = "__getdirentries64")] //special case for macos
        // Sneaky isnt it?, pretty much not seen this done anywhere before lol.
        fn getdirentries(fd: i32, buf: *mut c_char, nbytes: size_t, basep: *mut off_t) -> ssize_t;
    } // as above

    // SAFETY: As specified above
    unsafe { getdirentries(fd, buffer_ptr, nbytes, basep) }
}

/*


// Works same as above but I prefer to not rely on hardcoded syscall numbers! Old implementation, kept for posterity.
pub unsafe fn getdirentries64<T>(
    fd: libc::c_int,
    buffer_ptr: *mut T,
    nbytes: libc::size_t,
    basep: *mut libc::off_t,
) -> i32
where
    T: crate::fs::ValueType,
{
    const SYS_GETDIRENTRIES64: libc::c_int = 344; // Reverse engineered syscall number

    //https://phrack.org/issues/66/16
    // We verify this works via build script, we check if `getdirentries` returns >0 for tmp directory, if not, syscall is broken.
    unsafe { libc::syscall(SYS_GETDIRENTRIES64, fd, buffer_ptr, nbytes, basep) }
}
*/

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
        debug_assert!(!self.ends_with(b"/"), "file path ends with a slash!");
        // filepaths entering this will always have a slash in them, guaranteed, no trailing slashes!!!
        // The edge cases to watch our for are ./ and /, these are handled
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
        // (every file path going in here has at least a '/' inside it), this is a special case for root/'.'
        if self.len() == 1 {
            return 0;
        }

        debug_assert!(!self.is_empty(), "should never be empty");
        debug_assert!(!self.ends_with(b"/"), "file path ends with a slash!");
        debug_assert!(!self.is_empty(), "should never be empty");
        memrchr(b'/', self).map_or(1, |pos| pos + 1)
    }
}

#[inline]
#[must_use]
#[allow(clippy::missing_const_for_fn)]
/**
 Returns the length of `dirent64` / `dirent` `d_name` without the trailing null byte.

 On supported Unix targets this delegates to `dirent_const_time_strlen` for
 constant-time length detection; on other targets it falls back to
 `libc::strlen` on `d_name`. This will *always* take the most optimal route.

 # Safety
 - `drnt` must be a valid, non-null pointer to a `dirent` / `dirent64` whose `d_name`
   field is properly null-terminated within the record.
 - The pointer must remain valid for the duration of the call.
*/
pub unsafe fn dirent_name_length(drnt: *const dirent64) -> usize {
    debug_assert!(!drnt.is_null(), "dirent is null in name length calculation");
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "emscripten",
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
        unsafe { libc::strlen((&raw const (*drnt).d_name).cast()) }
        // Fallback for other OSes, strlen is either on i8 or u8, casting is 0 cast (it's essentially just reinterpreting)
        //Use raw const to take a pointer because the `d_name` isn't guaranteed to be [c_char;256] (variable length/unsized array)
        // EG for NTFS it can be up to 512 bytes
    }
}

/*
Const-time `strlen` for `dirent64's d_name` using SWAR bit tricks.
 (c) Alexander Curtis .
My Cat Diavolo is cute.




*/
// TODO! this only fails on solaris/illumos when going from root, WHY???? that makes no sense. I had to remove solaris/illumos support for this function. I am being too lazy to debug it
// I never came across the issue simply because I never tried searching from root on my VM, until today.... what a  weird bug JFC, I should investigate this if i feel like it.
// REALLY REALLY WEIRD
// nvm FOUND OUT WHY: d_reclen is 32 in /proc for illumos/solaris? WHY? this will never work on these systems due to this reason
// such a weird weird anomaly...

//cargo-asm --lib fdf::util::utils::dirent_const_time_strlen (put to inline(never) to display)

/**
 Returns the length of a `dirent64' /`dirent`  d_name` string in constant time using
 SWAR (SIMD within a register) bit tricks (equivalent to `libc::strlen`, does NOT include the null terminator)

 This function avoids branching and SIMD instructions, achieving O(1) time
 by reading the final 8 bytes of the structure and applying bit-masking
 operations to locate the null terminator.

 # Safety
 The caller must ensure:
 `dirent` is a valid, non-null pointer to a `libc::dirent64/libc::dirent`.

 # Performance
 This is almost always faster(by a significant amount) than strlen for dirents, expect in the case of trivially short names (potentially)
On some systems

 # Example
 ```

 #[cfg(any(target_os = "linux", target_os = "android"))]
 use libc::{dirent64,readdir64};

 #[cfg(not(any(target_os = "linux", target_os = "android")))]
 use libc::{readdir as readdir64,dirent as dirent64};


 use std::env::temp_dir;
 use std::fs;
 use std::os::unix::ffi::OsStrExt;
 use fdf::util::dirent_const_time_strlen;

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
    let mut entry = unsafe { readdir64(dir_fd) };
    while !entry.is_null() {
        let name_len = unsafe {
            dirent_const_time_strlen(entry as *const dirent64)
        };

        let actual_len = unsafe {
            libc::strlen((&raw const (*entry).d_name).cast())
        };
        assert_eq!(name_len, actual_len, "Const-time strlen matches libc strlen {name_len} {actual_len}");
        entry = unsafe { readdir64(dir_fd) };
    }
    unsafe { libc::closedir(dir_fd) };
 }

 fs::remove_dir_all(&target_path).ok();
 ```

 Notes: If using this on 32 bit, use `readdir64`/`getdents64`

 # References
 - [Stanford Bit Twiddling Hacks find 0 byte ](http://www.icodeguru.com/Embedded/Hacker%27s-Delight/043.htm)
 - [find crate `dirent.rs`](https://github.com/Soveu/find/blob/master/src/dirent.rs)
 - [Wojciech Muła ] (<http://0x80.pl/notesen/2016-11-28-simd-strfind.html#algorithm-1-generic-simd>)

*/
#[inline]
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "emscripten",
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
    clippy::host_endian_bytes,
    clippy::cast_ptr_alignment
)] //we're aligned (compiler can't see it though and we're doing fancy operations)
#[must_use]
pub const unsafe fn dirent_const_time_strlen(drnt: *const dirent64) -> usize {
    debug_assert!(!drnt.is_null(), "dirent is null in name length calculation");

    #[cfg(has_d_namlen)] //Generated by cc build script.
    // SAFETY: `dirent` must be validated ( it was required to not give an invalid pointer)
    return unsafe { (*drnt).d_namlen as usize }; //trivial operation for systems with d_namlen field
    #[cfg(not(has_d_namlen))]
    // On these systems where we need a bit of 'black magic' (no d_namlen field)
    {
        use core::num::NonZeroU64;
        // Offset from the start of the struct to the beginning of d_name.
        const DIRENT_HEADER_START: usize = core::mem::offset_of!(dirent64, d_name);
        // Access the last field and then round up to find the minimum struct size
        const MIN_DIRENT_SIZE: usize = DIRENT_HEADER_START.next_multiple_of(8);
        // Compile time assert to immediately cancel the build if invalidated
        const { assert!(MIN_DIRENT_SIZE == 24, "dirent min size must be 24!") };

        const LO_U64: u64 = u64::from_ne_bytes([0x01; size_of::<u64>()]);
        const HI_U64: u64 = u64::from_ne_bytes([0x80; size_of::<u64>()]);

        /*  SAFETY: `dirent` is valid by precondition */
        let reclen = unsafe { (*drnt).d_reclen } as usize;

        /*
          Read the last 8 bytes of the struct as a u64.
        This works because dirents are always 8-byte aligned. (it is guaranteed aligned by the kernel) */

        // SAFETY: We're indexing in bounds within the pointer. Since the reclen is size of the struct in bytes.
        let mut last_word: u64 = unsafe { drnt.byte_add(reclen - 8).cast::<u64>().read() };

        // Create a mask for the first 3 bytes in the case where reclen==24, this handles the big endian case too.

        const MASK: u64 = u64::from_ne_bytes([0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00]);
        /* When the record length is 24/`MIN_DIRENT_SIZE`, the kernel may insert nulls before d_name.
        Which will exist on index's 16/17/18  the d_name starts at 19, so anything before is invalid anyway.
        The index 16/17 will contauin the reclen, eg, for 24 it will simply be [24,0]
        the index 18 will contain the d_type, if it's unknown, then it'll be 0




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
        last_word |= mask;

        /*
          SWAR null detection algorithm:
         Convert each zero byte to 0x80 and non-zero bytes to 0x00 using bit tricks.
         This allows us to identify the position of the first null terminator in parallel.

         The formula: (candidate - 0x010101...) & ~candidate & 0x808080...
          - candidate - 0x01...: Creates 0xFF in bytes where candidate was 0x00
         - & ~candidate: Ensures we only mark bytes that were originally zero
          - & 0x80...: Isolates the high bit of each byte for null detection

           Check hackers delight reference above for better explanation.

          Then use a niche optimisation, because the last word will ALWAYS contain a null terminator,
          so we can use `NonZeroU64`!,
          This has the benefit of using a smarter intrinsic
          https://doc.rust-lang.org/src/core/num/nonzero.rs.html#599
        https://doc.rust-lang.org/beta/std/intrinsics/fn.ctlz_nonzero.html
        https://doc.rust-lang.org/beta/std/intrinsics/fn.cttz_nonzero.html

        This allows us to skip a 0 check which then allows us to use tzcnt/lzcnt on most cpu's (well x86_64, not knowledgeable on ARM/etc)
         */

        #[cfg(target_endian = "little")]
        //SAFETY: The u64 can never be all 0's post-SWAR
        let masked_word = unsafe {
            NonZeroU64::new_unchecked(last_word.wrapping_sub(LO_U64) & !last_word & HI_U64)
        };

        //http://0x80.pl/notesen/2016-11-28-simd-strfind.html#algorithm-1-generic-simd
        // ^ Reference for the BE algorithm
        // Use a borrow free algorithm to do this on BE safely(1 more instruction than LE)
        // This is overly precautious, mostly because we can't use the typical `HASZERO` due to the possible
        // present of 0x01 bytes in a filename, given POSIX paths are raw bytes
        // and the POSIX standard only dictates 1. a filename cannot contain a slash and 2. cannot be empty.
        #[cfg(target_endian = "big")]
        //SAFETY: The u64 can never be all 0's post-SWAR
        let masked_word = unsafe {
            NonZeroU64::new_unchecked(
                (!last_word & !HI_U64).wrapping_add(LO_U64) & (!last_word & HI_U64),
            )
        };

        // Find the position of the null terminator
        #[cfg(target_endian = "little")]
        let byte_pos = (masked_word.trailing_zeros() >> 3) as usize;
        #[cfg(target_endian = "big")]
        let byte_pos = (masked_word.leading_zeros() >> 3) as usize;

        //check final calculation
        debug_assert!(
            reclen - DIRENT_HEADER_START +byte_pos -8
                //SAFETY: should never matter because debug assert checks pointer validity above.
                    == unsafe{core::ffi::CStr::from_ptr((&raw const (*drnt).d_name).cast()).count_bytes() },
            // Use raw const to take a pointer because the `d_name` isn't guaranteed to be [c_char;256] (variable length array)
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
     assembly output: x86_64 with BMI/other optimisations

        movzx eax, word ptr [rdi + 16]
        xor ecx, ecx
        cmp rax, 24
        mov edx, 16777215
        cmovne rdx, rcx  <--- conditional move, no branch
        or rdx, qword ptr [rdi + rax - 8]
        movabs rcx, -72340172838076673 // Loading this constant with be amortised due to inlining (havent checked for stack spillage, eh, unavoidable if so anyways.)
        add rcx, rdx
        andn rcx, rdx, rcx
        movabs rdx, -9187201950435737472 // as above.
        and rdx, rcx
        tzcnt rcx, rdx
        shr ecx, 3
        lea rax, [rax + rcx - 27]
        ret


Without BMI

     movzx eax, word ptr [rdi + 16]
        xor ecx, ecx
        cmp rax, 24
        mov edx, 16777215
        cmovne rdx, rcx
        or rdx, qword ptr [rdi + rax - 8]
        movabs rcx, -72340172838076673
        add rcx, rdx
        not rdx
        and rcx, rdx
        movabs rdx, -9187201950435737472
        and rdx, rcx
        rep bsf rcx, rdx   <---- rep bsf is encoded to tzcnt on most x86_64 cpu's supporting it
        shr ecx, 3
        add rax, rcx
        add rax, -27
        ret


*/

/*

// Works on 32bit too, surprisingly!

// C implementation (so people can understand it better)

https://godbolt.org/z/9YM4xqx5s

#if defined(__linux__) && defined(__LP64__)
uint32_t dirent_const_time(const struct dirent *drnt) {
#define DIRENT_HEADER_START (offsetof(struct dirent, d_name))

#define MIN_DIRENT_SIZE (((DIRENT_HEADER_START) + 7) & ~7)
#define HI_U64 0x8080808080808080ULL
#define LO_U64 0x0101010101010101ULL

#if (__BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__)
#define MASK 0x0000000000FFFFFFULL
#else
#define MASK 0xFFFFFF0000000000ULL
#endif

  const uint32_t reclen = drnt->d_reclen;
  const uint64_t mask = MASK * (uint64_t)(reclen == MIN_DIRENT_SIZE);
  uint64_t last_word = *(uint64_t *)((uint8_t *)(drnt) + (reclen - 8));
  last_word |= mask;

#if (__BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__)
  const uint64_t masked_lasked_word =
      (last_word - LO_U64) & ~last_word & HI_U64;
  const uint32_t byte_pos = __builtin_ctzll(masked_lasked_word) >> 3;
#else
  const uint64_t masked_lasked_word =
      ((~last_word & ~HI_U64) + LO_U64) & (~last_word & HI_U64);
  const uint32_t byte_pos = __builtin_clzll(masked_lasked_word) >> 3;
#endif

  return reclen - DIRENT_HEADER_START + byte_pos - 8;
}
#else
#error "dirent_const_time is only supported on linux in this simplified example (and GCC/Clang, you'll need to use different intrinsics for MSVC (irrelevant cos wont work on windows lol)"
#endif
*/

/*
C version assembly
(Only distinction is we don't use LEA because we need to explicitly cast to usize in rust (for indexing))


dirent_const_time:
        movzx   ecx, WORD PTR [rdi+16]
        xor     edx, edx
        mov     eax, 16777215
        cmp     rcx, 24
        cmove   rdx, rax
        or      rdx, QWORD PTR [rdi-8+rcx]
        movabs  rax, -72340172838076673
        add     rax, rdx
        not     rdx
        and     rax, rdx
        movabs  rdx, -9187201950435737472
        and     rax, rdx
        rep bsf eax, eax
        shr     rax, 3
        lea     rax, [rcx-27+rax]
        ret

*/
