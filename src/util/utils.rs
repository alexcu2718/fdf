use crate::dirent64;
use core::ffi::{CStr, c_char, c_int, c_void};
/**
  Wrapper for direct getdents syscalls


 # Arguments
 - `fd`: Open directory file descriptor
 - `buffer_ptr`: Raw pointer to output buffer
 - `buffer_size`: Size of output buffer in bytes

 # Safety
 - Requires valid open directory descriptor
 - Buffer must be valid for writes of `buffer_size` bytes
 - Buffer must be aligned to 8 bytes.

 # Returns
 - Positive: Number of bytes read
 - 0: End of directory
 - Negative: Error code (set errno and check)
 - Buffer size must be less than `i32::MAX`

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
pub unsafe fn getdents64(fd: c_int, buffer_ptr: *mut c_void, buffer_size: usize) -> isize {
    const { assert!(libc::INT_MAX == i32::MAX, "Trivial assert") }; //paranoia
    debug_assert!(!buffer_ptr.is_null(), "Buffer  is null in getdents64");
    debug_assert!(buffer_ptr.addr().is_multiple_of(8), "Buf not aligned to 8");

    //https://github.com/bminor/glibc/blob/04e750e75b73957cf1c791535a3f4319534a52fc/sysdeps/unix/sysv/linux/getdents64.c#L30
    debug_assert!(
        buffer_size <= libc::INT_MAX as _,
        "buffer_size passed to getdents64 too big"
    );
    #[cfg(any(
        target_os = "openbsd",
        target_os = "solaris",
        target_os = "illumos",
        target_os = "netbsd"
    ))]
    {
        //Link the function, we can't use the direct syscall because BSD's dont allow it.
        unsafe extern "C" {
            //TODO add dragonfly here(?) TODO once they support Rust 2024
            #[cfg_attr(target_os = "netbsd", link_name = "__getdents30")] //special case for NetBSD
            //#[cfg_attr(any(target_os = "linux", target_os = "android"),link_name = "getdents64")]
            // ^ how to link on Linux, not sure if this works on android though., too lazy to test.
            fn getdents(fd: c_int, dirp: *mut c_void, count: usize) -> isize;
        }
        // SAFETY: non required except buffer length must not exceed the provided size, then we get accessing out of bounds memory...
        unsafe { getdents(fd, buffer_ptr, buffer_size) }
    }

    // SAFETY: Syscall has no other implicit safety requirements beyond pointer validity(and precursor conditions met.)
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[expect(clippy::cast_possible_truncation, reason = "clong is isize on Unix")]
    unsafe {
        libc::syscall(libc::SYS_getdents64, fd, buffer_ptr, buffer_size) as _
    } // We can do similar linking for getdents64 but prefer not to use the indirection if can be avoided.
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
 - Buffer must be aligned to 8 bytes.

 # Returns
 - Positive: Number of bytes read
 - 0: End of directory
 - Negative: Error code (check errno)
 - Buffer size must be less than `i32::MAX`


 This function is only available on macOS/FreeBSD
*/
pub unsafe fn getdirentries64(
    fd: c_int,
    buffer_ptr: *mut c_void,
    nbytes: usize,
    basep: *mut libc::off_t,
) -> isize {
    const { assert!(libc::INT_MAX == i32::MAX, "Trivial assert") }; // me being overly paranoid.
    debug_assert!(!buffer_ptr.is_null(), "Buffer  is null in GDE64");
    debug_assert!(buffer_ptr.addr().is_multiple_of(8), "Buf not aligned to 8");
    debug_assert!(
        nbytes <= libc::INT_MAX as _,
        "Buffer passed to getdirentries64 too big"
    );
    // link to libc
    unsafe extern "C" {
        #[cfg_attr(
            all(target_os = "macos", target_pointer_width = "64"),
            link_name = "__getdirentries64"
        )] //special case for macos
        // Never seen this done, I searched all of github for similar stuff. I love dirty stuff like this.
        fn getdirentries(
            fd: c_int,
            buf: *mut c_void,
            nbytes: usize,
            basep: *mut libc::off_t,
        ) -> isize;
    } // as above
    // By doing this, we avoid fstatf64 calls and a thread mutex enforced by readdir (completely not needed for single thread reading)
    // IT MAKES NO SENSE to parallelise readdir, it's fundamentally a sequential operation unless you're doing some really wacky stuff.
    // https://github.com/apple-oss-distributions/Libc/blob/899a3b2d52d95d75e05fb286a5e64975ec3de757/gen/FreeBSD/opendir.c#L334
    // SAFETY: As specified above
    unsafe { getdirentries(fd, buffer_ptr, nbytes, basep) }
}

/*
For macos you can also do this but linking the symbol is much better/safer
  const SYS_GETDIRENTRIES64: libc::c_int = 344; // Reverse engineered syscall number
    //https://phrack.org/issues/66/16

    unsafe { libc::syscall(SYS_GETDIRENTRIES64, fd, buffer_ptr, nbytes, basep) }
*/

#[cfg(target_os = "macos")]
unsafe extern "C" {
    #[link_name = "openat$NOCANCEL"]
    /// Skip cancellation points in multhithreaded code.
    pub(crate) unsafe fn openat_nocancel(dirfd: c_int, path: *const c_char, flags: c_int) -> c_int;
}

// https://users.rust-lang.org/t/compiler-hint-for-unlikely-likely-for-if-branches/62102/4
// copied from hashbrown
#[inline]
#[cold]
const fn cold() {}

#[inline]
pub(crate) const fn unlikely(b: bool) -> bool {
    if b {
        cold()
    }
    b
}

#[inline]
#[must_use]
/**
 Returns the length of `dirent64` / `dirent` `d_name` without the trailing null byte.

 On supported Unix targets this delegates to `dirent_const_time_strlen` for
 constant-time length detection; on other targets it falls back to
 `libc::strlen` on `d_name`. This will *always* take the most optimal route.

 # Safety
 - `drnt` must be a valid, non-null pointer to a `dirent` / `dirent64` whose `d_name`
   field is properly null-terminated within the record.
 - The pointer must remain valid for the duration of the call.
 - dirent64 must be kernel provided, so from 'readdir(64)' or appropriate `syscall`
*/
pub const unsafe fn dirent_name_length(drnt: *const dirent64) -> usize {
    debug_assert!(!drnt.is_null(), "dirent is null in name length calculation");
    #[cfg(any(target_os = "linux", target_os = "android", has_d_namlen))]
    {
        // SAFETY: `dirent` must be checked before hand to not be null
        unsafe { dirent_const_time_strlen(drnt) }
    }

    #[cfg(not(any(target_os = "linux", target_os = "android", has_d_namlen)))]
    {
        // The above has the same assembly as below but the below is allowed in const context.
        // SAFETY: `dirent` must be checked before hand to not be null
        unsafe { strlen((&raw const (*drnt).d_name).cast()) }
        //Use raw const to take a pointer because the `d_name` isn't guaranteed to be [c_char;256] (variable length/unsized array)
        // EG for NTFS it can be up to 512 bytes
    }
}

/**
A convenience const function which is *just* a fancy call to your libc's strlen.
*/
#[inline(always)]
#[expect(clippy::inline_always, reason = "Allow codegen to see strlen")]
pub(crate) const unsafe fn strlen(x: *const c_char) -> usize {
    // SAFETY: user has to check whether pointer is null
    unsafe { CStr::from_ptr(x).count_bytes() }
    // equivalent assembly to
    // unsafe{libc::strlen(x)}
}

#[allow(clippy::undocumented_unsafe_blocks)] //stupid lints.
const _: () = assert!(unsafe { strlen(c"hello".as_ptr()) } == 5, "removing lint");

// this only fails on solaris/illumos when going from root, WHY???? that makes no sense. I had to remove solaris/illumos support for this function.
// I never came across the issue simply because I never tried searching from root on my VM, until today...
// FOUND OUT WHY: d_reclen is 32 in /proc for illumos/solaris for small files. WHY? this will never work on these systems due to this reason
// leaving this here as a warning to all, don't assume too much, test! (IT )
// such a weird weird anomaly... (It probably holds kernel metadata or something, not booting up my solaris VM to test as all CI tests pass.)

/*
Const-time `strlen` for `dirent64's d_name` using SWAR bit tricks.
 (c) Alexander Curtis .
My Cat Diavolo is cute.
*/

/**
 Returns the length of a `dirent64' /`dirent`  d_name` string in constant time using
 SWAR (SIMD within a register) bit tricks (equivalent to `libc::strlen`, does NOT include the null terminator)

 This function avoids branching and SIMD instructions, achieving O(1) time (Well, nearly, slight cache effects but mostly unmeasurable)
 by reading the final 8 bytes of the structure and applying bit-masking
 operations to locate the null terminator.

 # Safety
 The caller must ensure:
 `dirent` is a valid, non-null pointer to a `libc::dirent64/libc::dirent`.
 The minimum reclen is 24, which on a non-corrupted filesystem is perfectly reasonable, if you have a corrupted filesystem, good luck!

 # Performance
 This is almost always faster(by a significant amount) than strlen for dirents, I have benchmarked it and it's in the cargo bench.
 Mostly because strlen requires a non inlineable function call to libc so even for trivial lengths, the call overhead will dominate.

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
#[cfg(any(target_os = "linux", target_os = "android", has_d_namlen))]
// we can add more systems here but they're obscure, ie hermit/fuschia etc
// given I lack tests for these, I will only add if needed. Fuschia/Hermit/bunch of others will likely work
// but it's a pain to make a VM, they probably don't support rust 2024 either...
#[must_use]
pub const unsafe fn dirent_const_time_strlen(drnt: *const dirent64) -> usize {
    debug_assert!(!drnt.is_null(), "dirent is null in name length calculation");

    #[cfg(has_d_namlen)] //Generated by cc build script.
    // SAFETY: `dirent` must be validated ( it was required to not give an invalid pointer)
    return unsafe { (*drnt).d_namlen as usize }; //trivial operation for systems with d_namlen field
    #[cfg(not(has_d_namlen))]
    // On these systems where we need a bit of 'black magic' (no d_namlen field)
    {
        use core::{mem::offset_of, num::NonZeroU64};
        // Offset from the start of the struct to the beginning of d_name.
        const DIRENT_HEADER_START: usize = offset_of!(dirent64, d_name);
        // Access the last field and then round up to find the minimum struct size
        const MIN_DIRENT_SIZE: usize = DIRENT_HEADER_START.next_multiple_of(8);
        // Compile time assert to immediately cancel the build if invalidated
        const { assert!(MIN_DIRENT_SIZE == 24, "dirent min size must be 24!") };
        const { assert!(align_of::<dirent64>() == align_of::<u64>(), " not aligned!") };
        const LO_U64: u64 = 0x0101_0101_0101_0101;
        const HI_U64: u64 = 0x8080_8080_8080_8080;

        /*  SAFETY: `dirent` is valid by precondition */
        let reclen = unsafe { (*drnt).d_reclen } as usize;
        debug_assert!(reclen.is_multiple_of(8), "reclen not % 8==0");
        debug_assert!(reclen >= 24, "reclen must be >=24, likely a corrupted fs");

        /*
          Read the last 8 bytes of the struct as a u64.
        This works because dirents are always 8-byte aligned. (it is guaranteed aligned by the kernel) */

        // SAFETY: We're indexing in bounds within the pointer. Since the reclen is size of the struct in bytes.
        // and above.
        let mut last_word: u64 = unsafe { drnt.byte_add(reclen - 8).cast::<u64>().read() };

        // Create a mask for the first 3 bytes in the case where reclen==24, this handles the big endian case too.
        /* When the record length is 24/`MIN_DIRENT_SIZE`, the kernel may insert nulls before d_name.
        Which will exist on index's 16/17/18  the d_name starts at 19, so anything before is invalid anyway.
        The index 16/17 will contain the reclen, eg, for 24 it will simply be [24,0], if 256 it'll be [0,1]
        the index 18 will contain the d_type, if it's unknown, then it'll be 0

        Mask them out to avoid false detection of a terminator.*/

        /*
            This hacky expression generates a 24-bit mask without using a comparison or branch.

            For the minimum valid directory-entry size(24), subtracting 25 underflows by one.
            Because the subtraction is wrapping and performed as a u64, the result becomes u64::MAX:

            shifting this right by 40 bits leaves exactly the low 24 bits set:
        */
        #[cfg(target_endian = "little")]
        let mask = (reclen as u64).wrapping_sub(25) >> 40;

        // Big endian has the bits in the correct position however we only want the first 3 bytes.
        #[cfg(target_endian = "big")]
        let mask = (reclen as u64).wrapping_sub(25) & 0xFFFF_FF00_0000_0000; // Could cast to a u32, shift by 8 then cast back to u64 but that's horrible
        // not checking the asm on that...
        debug_assert!(
            // handy debug test, explains what the above means!
            reclen == 24 && mask == u64::from_ne_bytes([0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0])
                || mask == 0 && reclen != 24,
            "Checking condition holds"
        );

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

        This allows us to skip a 0 check which then allows us to use tzcnt/lzcnt on x86_64 (most platforms using this are probably x86_65)
        because although rep bsf is microcoded to tzcnt (if supported), it still has to do an unnecessary 0 check here
         */

        //SAFETY: The u64 can never be all 0's post-mask because the last word ALWAYS contains at least one NUL, which become 0x80
        #[cfg(target_endian = "little")]
        let masked_word = unsafe {
            NonZeroU64::new_unchecked(last_word.wrapping_sub(LO_U64) & !last_word & HI_U64)
        };

        //http://0x80.pl/notesen/2016-11-28-simd-strfind.html#algorithm-1-generic-simd
        // ^ Reference for the BE algorithm
        // Use a borrow free algorithm to do this on BE safely(2 more instruction than LE)
        // This is overly precautious, mostly because we can't use the typical `HASZERO` due to the possible
        // present of 0x01 bytes in a filename, given POSIX paths are raw bytes
        // and the POSIX standard only dictates 1. a filename cannot contain a slash and 2. cannot be empty.
        #[cfg(target_endian = "big")]
        //SAFETY: as in LE version.
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
                    == unsafe{strlen((&raw const (*drnt).d_name).cast()) },
            // Use raw const to take a pointer because the `d_name` isn't guaranteed to be [c_char;256] (variable length array)
            // We use this method as workaround specifically because `from_ptr` is const
            // (We could've also used a while loop but that's even more verbose and makes this more complicated....)
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


fdf::util::utils::dirent_const_time_strlen:
        movzx eax, word ptr [rdi + 16]
        lea rcx, [rax - 25]
        shr rcx, 40
        or rcx, qword ptr [rdi + rax - 8]
        movabs rdx, -72340172838076673  ;Loading this constant with be amortised due to inlining (havent checked for stack spillage, eh, unavoidable if so anyways.)
        add rdx, rcx
        andn rcx, rcx, rdx
        movabs rdx, -9187201950435737472
        and rdx, rcx
        tzcnt rcx, rdx
        shr ecx, 3
        lea rax, [rax + rcx - 27]
        ret


Without BMI

       movzx eax, word ptr [rdi + 16]
        lea rcx, [rax - 25]
        shr rcx, 40
        or rcx, qword ptr [rdi + rax - 8]
        movabs rdx, -72340172838076673
        add rdx, rcx
        not rcx
        and rdx, rcx
        movabs rcx, -9187201950435737472
        and rcx, rdx
        rep bsf rcx, rcx
        shr ecx, 3
        add rax, rcx
        add rax, -27
        ret


*/
