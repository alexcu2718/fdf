//! `dirent64` specific accessors for [`Unique`]
#![expect(clippy::inline_always, reason = "just a wrapper")]
use crate::{Unique, dirent64};
use core::{
    ffi::{CStr, c_char},
    slice,
};

impl Unique<dirent64> {
    #[inline(always)]
    #[must_use]
    /// Returns the inode of the `dirent64`, returns 0 if `d_ino` is not a struct member on your OS.
    /// # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn d_ino(&self) -> u64 {
        // SAFETY: caller must maintain the invariant

        #[cfg(all(
            any(
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "dragonfly"
            ),
            has_d_ino
        ))]
        {
            // SAFETY: checked by caller
            unsafe { (*self.as_ptr()).d_fileno as _ }
        }

        #[cfg(all(
            not(any(
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "dragonfly"
            )),
            has_d_ino
        ))]
        {
            // SAFETY: checked by caller
            unsafe { (*self.as_ptr()).d_ino as _ }
            //irritating.
        }

        #[cfg(not(has_d_ino))]
        {
            0
        }
    }

    #[inline(always)]
    #[must_use]
    /// Returns the libc equivalent of the type as eg `DT_BLK`  from libc
    /// Follow the link  for [GNU documentation ](<https://ftp.gnu.org/old-gnu/Manuals/glibc-2.2.3/html_node/libc_260.html>)
    ///
    /// Returns `DT_UNKNOWN` for systems without `d_type` (eg Solaris/Illumos)
    ///
    /// # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn d_type(&self) -> u8 {
        #[cfg(has_d_type)]
        // SAFETY: caller must maintain the invariant
        unsafe {
            (*self.as_ptr()).d_type as _
        }

        #[cfg(not(has_d_type))]
        libc::DT_UNKNOWN
    }

    #[must_use]
    #[inline(always)]
    #[cfg(has_d_namlen)]
    /// Returns the `d_namlen` if the member exists on your system
    /// # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn d_namlen(&self) -> usize {
        // SAFETY: caller must maintain the invariant
        unsafe { (*self.as_ptr()).d_namlen as _ }
    }

    #[must_use]
    #[inline(always)]
    #[cfg(not(has_d_namlen))]
    #[deprecated(
        note = "`d_namlen` does not exist on this platform's `dirent`; use `name_length()` instead, which is portable"
    )]
    /// Stub present only to give a clear diagnostic; this platform has no `d_namlen` field.
    /// # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn d_namlen(&self) -> usize {
        // SAFETY: caller must maintain the invariant
        unsafe { self.name_length() }
    }

    #[must_use]
    #[inline(always)]
    /// Returns a slice reference to the `d_name`, not including null terminator.
    /// # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn d_name_slice(&self) -> &[u8] {
        // SAFETY: Caller must not invalidate the invariant.
        unsafe { slice::from_raw_parts(self.d_name().cast(), self.name_length()) }
    }

    #[must_use]
    #[inline(always)]
    /// Returns a `CStr` reference to the `d_name`, (including null terminator.)
    /// # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn d_name_cstr(&self) -> &CStr {
        // SAFETY: caller must ensure the invariants
        let cslice = unsafe { slice::from_raw_parts(self.d_name().cast(), self.name_length() + 1) };
        // add  1 to include NUL
        // SAFETY:  has it's null terminator included, and no interior nulls..
        unsafe { CStr::from_bytes_with_nul_unchecked(cslice) } //equivalent if you cast it.
    }

    #[inline(always)]
    #[must_use]
    #[cfg(has_d_reclen)] // Overly paranoid, I want to know what builds fail.
    /// Returns the `d_reclen` member as a usize.
    ///  # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn d_reclen(&self) -> usize {
        //SAFETY: TRIVIALLY VALID BY CONSTRUCTION

        let reclen = unsafe { (*self.as_ptr()).d_reclen as usize };
        // although most systems should have this struct member.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        debug_assert!(reclen >= 24, "expected reclen >=24");
        reclen
    }

    #[inline(always)]
    #[must_use]
    // can be anything from [c_char;3] to [c_char;1024] or whatever. It's a VLA.
    /**
    Access the pointer to `d_name`,

    Note that `d-name` is not a [ `c_char` ;256] array, it can be greater,
    So careful attention has to be paid

     # Safety
    The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    */
    pub const unsafe fn d_name(&self) -> *const c_char {
        // SAFETY: caller must maintain the invariant
        unsafe { (&raw const (*self.as_ptr()).d_name).cast() }
    }

    #[inline(always)]
    #[must_use]
    /// Returns the length of the `d_name` but, like strlen, it doesn't include the null terminator.
    /// # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn name_length(&self) -> usize {
        // SAFETY: caller must maintain the invariant
        unsafe { crate::util::dirent_name_length(self.as_ptr()) }
    }
}
