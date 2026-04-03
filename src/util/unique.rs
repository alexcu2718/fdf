/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This crate provides an [`Unique`] implementation
//! without using any unstable code that would require
//! nightly.
//!
//! Look at the [`Unique` definition](crate::Unique) for more info.
#![allow(clippy::missing_inline_in_public_items)]
use crate::dirent64;
use core::convert::From;
use core::fmt;
use core::marker::PhantomData;
use core::ptr::NonNull;

/// A wrapper around a raw non-null `*mut T` that indicates that the possessor
/// of this wrapper owns the referent. Useful for building abstractions like
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

impl<T: Sized> Unique<T> {
    /// Creates a new `Unique` that is dangling, but well-aligned.
    ///
    /// This is useful for initializing types which lazily allocate, like
    /// `Vec::new` does.
    ///
    /// Note that the pointer value may potentially represent a valid pointer to
    /// a `T`, which means this must not be used as a "not yet initialized"
    /// sentinel value. Types that lazily allocate must track initialization by
    /// some other means.
    #[inline]
    #[must_use]
    pub const fn dangling() -> Self {
        // SAFETY: mem::align_of() returns a valid, non-null pointer. The
        // conditions to call new_unchecked() are thus respected.
        unsafe { Self::new_unchecked(core::ptr::dangling_mut::<T>()) }
    }
}

impl<T: ?Sized> Unique<T> {
    /// Creates a new `Unique`.
    ///
    /// # Safety
    ///
    /// `ptr` must be non-null.
    #[inline]
    pub const unsafe fn new_unchecked(ptr: *mut T) -> Self {
        // SAFETY: the caller must guarantee that `ptr` is non-null.
        unsafe { Self(NonNull::new_unchecked(ptr), PhantomData) }
    }

    /// Creates a new `Unique` if `ptr` is non-null.
    #[inline]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub const fn new(ptr: *mut T) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            // SAFETY: The pointer has already been checked and is not null.
            Some(unsafe { Self::new_unchecked(ptr) })
        }
    }

    /// Acquires the underlying `*const` pointer.
    #[inline]
    #[must_use]
    pub const fn as_ptr(self) -> *const T {
        self.0.as_ptr().cast_const()
    }

    #[inline]
    #[must_use]
    pub const fn as_mut_ptr(self) -> *mut T {
        self.0.as_ptr()
    }

    /// Dereferences the content.
    ///
    /// The resulting lifetime is bound to self so this behaves "as if"
    /// it were actually an instance of T that is getting borrowed. If a longer
    /// (unbound) lifetime is needed, use `&*my_ptr.as_ptr()`.
    /// # Safety
    /// the caller must guarantee that this object meets all the meets all the
    /// requirements for a reference.
    #[inline]
    #[must_use]
    pub const unsafe fn as_ref(&self) -> &T {
        // SAFETY: the caller must guarantee that `self` meets all the
        // requirements for a reference.
        unsafe { &*self.as_ptr() }
    }

    /// Casts to a pointer of another type.
    #[inline]
    #[must_use]
    pub const fn cast<U>(self) -> Unique<U> {
        // SAFETY: Unique::new_unchecked() creates a new unique and needs
        // the given pointer to not be null.
        // Since we are passing self as a pointer, it cannot be null.
        unsafe { Unique::<U>::new_unchecked(self.as_ptr() as _) }
    }
}

impl<T: ?Sized> Clone for Unique<T> {
    #[inline]
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

impl<T: ?Sized> From<&mut T> for Unique<T> {
    #[inline]
    fn from(reference: &mut T) -> Self {
        // SAFETY: A mutable reference cannot be null
        unsafe { Self::new_unchecked(core::ptr::from_mut(reference)) }
    }
}

impl<T: ?Sized> From<Unique<T>> for NonNull<T> {
    #[inline]
    fn from(unique: Unique<T>) -> Self {
        unique.0
    }
}

impl Unique<dirent64> {
    #[inline]
    #[must_use]
    pub const fn d_ino(&self) -> u64 {
        // SAFETY: TRIVIALLY VALID BY CONSTRUCTION
        unsafe { access_dirent!(self.0.as_ptr(), d_ino) }
    }

    #[inline]
    #[must_use]
    pub const fn d_type(&self) -> u8 {
        // SAFETY: TRIVIALLY VALID BY CONSTRUCTION
        unsafe { access_dirent!(self.0.as_ptr(), d_type) }
    }

    #[inline]
    #[must_use]
    pub const fn d_name(&self) -> *const crate::c_char {
        // SAFETY: TRIVIALLY VALID BY CONSTRUCTION
        unsafe { access_dirent!(self.0.as_ptr(), d_name) }
    }

    #[inline]
    #[must_use]
    pub fn name_length(&self) -> usize {
        // SAFETY: TRIVIALLY VALID BY CONSTRUCTION
        unsafe { crate::util::dirent_name_length(self.as_ptr()) }
    }
}
