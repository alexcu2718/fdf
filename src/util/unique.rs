/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/*

 source taken from https://docs.rs/unique/latest/src/unique/lib.rs.html#47

Edited slightly, essentially providing a type safe wrapper to inforce internal invariants rather than rely on `faith`



 */
//! This crate provides an [`Unique`] implementation
//! without using any unstable code that would require
//! nightly.
//!
//! Look at the [`Unique` definition](crate::fdf) for more info.
#![allow(clippy::missing_inline_in_public_items)]
#![expect(clippy::inline_always, reason = "just a wrapper")]
use crate::dirent64;
use core::convert::From;
use core::ffi::{CStr, c_char};
use core::fmt;
use core::marker::PhantomData;
use core::num::NonZeroUsize;
use core::ptr::NonNull;
use core::slice::from_raw_parts;

/// A wrapper around a raw non-null `*const T` that indicates that the possessor
/// of this wrapper owns the referent. Usefor building abstractions like
/// `Box<T>`, `Vec<T>`, `String`, and `HashMap<K, V>`.
///
/// Unlike `*mut T`, `Unique<T>` behaves "as if" it were an instance of `T`.
/// It implements `Send`/`Sync` if `T` is `Send`/`Sync`. It also implies
/// the kind of strong aliasing guarantees an instance of `T` can expect:
/// the referent of the pointer should not be modified without a unique path to
/// its owning Unique.
///
/// If you're uncertain of whether it's correct to use `Unique` for your purposes,
/// consider using `NonNull`, which has weaker semantics.
///
/// Unlike `*mut T`, the pointer must always be non-null, even if the pointer
/// is never dereferenced. This is so that enums may use this forbidden value
/// as a discriminant -- `Option<Unique<T>>` has the same size as `Unique<T>`.
/// However the pointer may still dangle if it isn't dereferenced.
///
/// ```rust
/// use fdf::Unique;
/// assert_eq!(size_of::<Unique<u8>>(), size_of::<Option<Unique<u8>>>());
/// assert_eq!(size_of::<Unique<u8>>(), size_of::<*const u8>());
/// assert_eq!(size_of::<Unique<usize>>(), size_of::<usize>());
/// ```
///
/// Unlike `*mut T`, `Unique<T>` is covariant over `T`. This should always be correct
/// for any type which upholds Unique's aliasing requirements.
#[repr(transparent)]
pub struct Unique<T: ?Sized>(NonNull<T>, PhantomData<T>);

/// SAFETY: `Unique` pointers are `Send` if `T` is `Send` because the data they
/// reference is unaliased. Note that this aliasing invariant is
/// unenforced by the type system; the abstraction using the
/// `Unique` must enforce it.
unsafe impl<T: Send + ?Sized> Send for Unique<T> {}

/// SAFETY: `Unique` pointers are `Sync` if `T` is `Sync` because the data they
/// reference is unaliased. Note that this aliasing invariant is
/// unenforced by the type system; the abstraction using the
/// `Unique` must enforce it.
unsafe impl<T: Sync + ?Sized> Sync for Unique<T> {}

impl<T> Unique<T> {
    #[inline(always)]
    #[must_use]
    pub fn is_aligned(self) -> bool {
        self.as_non_null().is_aligned()
    }

    /// Creates a new `Unique` that is dangling, but well-aligned.
    ///
    /// This is useful for initializing types which lazily allocate, like
    /// `Vec::new` does.
    ///
    /// Note that the pointer value may potentially represent a valid pointer to
    /// a `T`, which means this must not be used as a "not yet initialized"
    /// sentinel value. Types that lazily allocate must track initialization by
    /// some other means.
    #[inline(always)]
    #[must_use]
    pub const fn dangling() -> Self {
        Self::from_non_null(NonNull::dangling())
    }

    #[inline(always)]
    #[must_use]
    pub fn is_aligned_to<U>(self) -> bool {
        self.cast::<U>().is_aligned()
    }
}

impl<T: ?Sized> Unique<T> {
    /// Creates a new `Unique`.
    ///
    /// # Safety
    ///
    /// `ptr` must be non-null.
    #[inline(always)]
    pub const unsafe fn new_unchecked(ptr: *const T) -> Self {
        debug_assert!(!ptr.is_null(), "Pointer is null in Unique::new_unchecked");
        // SAFETY: the caller must guarantee that `ptr` is non-null.
        unsafe { Self::from_non_null(NonNull::new_unchecked(ptr.cast_mut())) }
    }
    #[inline(always)]
    #[must_use]
    /// <https://doc.rust-lang.org/beta/std/ptr/struct.NonNull.html#method.byte_add>
    ///
    /// # Safety
    ///
    /// Same requirements as [`NonNull::byte_add`].
    pub const unsafe fn byte_add(self, amt: usize) -> Self {
        // SAFETY: same requirements as normal byte add, checked by caller.
        unsafe { Self::from_non_null(self.as_non_null().byte_add(amt)) }
    }

    #[inline(always)]
    #[must_use]
    /// <https://doc.rust-lang.org/beta/std/ptr/struct.NonNull.html#method.byte_sub>
    ///
    /// # Safety
    ///
    /// Same requirements as [`NonNull::byte_sub`].
    pub const unsafe fn byte_sub(self, amt: usize) -> Self {
        // SAFETY: same requirements as normal byte_sub, checked by caller
        unsafe { Self::from_non_null(self.as_non_null().byte_sub(amt)) }
    }

    #[inline(always)]
    pub const fn from_ref(reff: &T) -> Self {
        Self::from_non_null(NonNull::from_ref(reff))
    }

    #[inline(always)]
    pub const fn from_mut_ref(mreff: &mut T) -> Self {
        Self::from_ref(mreff)
    }

    #[inline(always)]
    #[must_use]
    pub fn addr(self) -> NonZeroUsize {
        self.as_non_null().addr()
    }

    /// Creates a new `Unique` if `ptr` is non-null.
    #[inline(always)]
    pub const fn new(ptr: *const T) -> Option<Self> {
        // manual map(const)
        match NonNull::new(ptr.cast_mut()) {
            Some(num) => Some(Self(num, PhantomData)),
            _ => None,
        }
    }

    /// Acquires the underlying `*const` pointer.
    #[inline(always)]
    #[must_use]
    pub const fn as_ptr(self) -> *const T {
        self.0.as_ptr()
    }

    #[inline(always)]
    #[must_use]
    pub const fn as_non_null(self) -> NonNull<T> {
        self.0
    }

    #[inline(always)]
    #[must_use]
    pub const fn from_non_null(ptr: NonNull<T>) -> Self {
        Self(ptr, PhantomData)
    }

    /// Dereferences the content.
    ///
    /// The resulting lifetime is bound to self so this behaves "as if"
    /// it were actually an instance of T that is getting borrowed. If a longer
    /// (unbound) lifetime is needed, use `&*my_ptr.as_ptr()`.
    /// # Safety
    /// the caller must guarantee that this object meets all the meets all the
    /// requirements for a reference.
    #[inline(always)]
    #[must_use]
    pub const unsafe fn as_ref<'ptr>(self) -> &'ptr T {
        // SAFETY: the caller must guarantee that `self` meets all the
        // requirements for a reference.
        unsafe { &*self.as_ptr() }
    }

    /// Casts to a pointer of another type.
    #[inline(always)]
    #[must_use]
    pub const fn cast<U>(self) -> Unique<U> {
        // the given pointer to not be null.
        // Since we are passing self as a pointer, it cannot be null.
        Unique::from_non_null(self.as_non_null().cast())
    }
}

impl<T: ?Sized> Clone for Unique<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for Unique<T> {}

impl<T: ?Sized> fmt::Debug for Unique<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&self.as_ptr(), f)
    }
}

impl<T: ?Sized> fmt::Pointer for Unique<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&self.as_ptr(), f)
    }
}

impl<T: ?Sized> From<&T> for Unique<T> {
    #[inline(always)]
    fn from(reference: &T) -> Self {
        Self::from_ref(reference)
    }
}

impl<T: ?Sized> From<&mut T> for Unique<T> {
    #[inline(always)]
    fn from(reference: &mut T) -> Self {
        Self::from_mut_ref(reference)
    }
}

impl<T: ?Sized> From<Unique<T>> for NonNull<T> {
    #[inline(always)]
    fn from(unique: Unique<T>) -> Self {
        unique.as_non_null()
    }
}

impl Unique<dirent64> {
    #[inline(always)]
    #[must_use]
    /// Returns the inode of the `dirent64`, returns 0 if `d_ino` is not a struct member on your OS.
    /// # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn d_ino(self) -> u64 {
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
    pub const unsafe fn d_type(self) -> u8 {
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
    pub const unsafe fn d_namlen(self) -> usize {
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
    pub const unsafe fn d_namlen(self) -> usize {
        // SAFETY: caller must maintain the invariant
        unsafe { self.name_length() }
    }

    #[must_use]
    #[inline(always)]
    /// Returns a slice reference to the `d_name`, not including null terminator.
    /// # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn d_name_slice<'pointer>(self) -> &'pointer [u8] {
        // SAFETY: Caller must not invalidate the invariant.
        unsafe { from_raw_parts(self.d_name().cast(), self.name_length()) }
    }

    #[must_use]
    #[inline(always)]
    /// Returns a `CStr` reference to the `d_name`, (including null terminator.)
    /// # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn d_name_cstr<'pointer>(self) -> &'pointer CStr {
        // SAFETY: caller must ensure the invariants
        let slice: &[u8] = unsafe { from_raw_parts(self.d_name().cast(), self.name_length() + 1) }; // add  1 to include terminal

        // SAFETY:  has it's null terminator included, and no interior nulls..
        unsafe { CStr::from_bytes_with_nul_unchecked(slice) } //equivalent if you cast it.
    }

    #[inline(always)]
    #[must_use]
    #[cfg(has_d_reclen)] // Overly paranoid, I want to know what builds fail.
    /// Returns the `d_reclen` member as a usize.
    ///  # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn d_reclen(self) -> usize {
        //SAFETY: TRIVIALLY VALID BY CONSTRUCTION

        unsafe { (*self.as_ptr()).d_reclen as _ }
        // although most systems should have this struct member.
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
    pub const unsafe fn d_name(self) -> *const c_char {
        // SAFETY: caller must maintain the invariant
        unsafe { (&raw const (*self.as_ptr()).d_name).cast() }
    }

    #[inline(always)]
    #[must_use]
    /// Returns the length of the `d_name` but, like strlen, it doesn't include the null terminator.
    /// # Safety
    /// The pointer must be valid and not invalidated by another `readdir`/`getdents(64)` call
    pub const unsafe fn name_length(self) -> usize {
        // SAFETY: caller must maintain the invariant
        unsafe { crate::util::dirent_name_length(self.as_ptr()) }
    }
}
