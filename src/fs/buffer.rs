use core::fmt::{Debug, Display};
use core::marker::Copy;
use core::mem::MaybeUninit;
use core::ops::{Add, Div, Mul, Sub};
#[allow(unused)]
use std::os::fd::{AsRawFd as _, BorrowedFd};
mod sealed {
    /// Sealed trait pattern to restrict `ValueType`
    pub trait Sealed {}
    impl Sealed for i8 {}
    impl Sealed for u8 {}
    impl Sealed for u64 {}
    impl Sealed for i64 {}
    impl Sealed for usize {}
    impl Sealed for isize {}
    impl Sealed for i32 {}
    impl Sealed for u32 {}
    impl Sealed for i16 {}
    impl Sealed for u16 {}
}

/// Marker trait for valid buffer value types
///
/// This trait ensures type safety while allowing the buffer to work with both
/// signed and unsigned byte types, which are equivalent for raw memory operations.
pub trait ValueType:
    sealed::Sealed + Copy + Debug + Display + Add + Sub + Mul + Div + Default
{
}

impl ValueType for i8 {}
impl ValueType for u8 {}
impl ValueType for u64 {}
impl ValueType for i64 {}
impl ValueType for usize {}
impl ValueType for isize {}
impl ValueType for i32 {}
impl ValueType for u32 {}

/**
 A optimised, aligned buffer for system call operations

 This buffer provides memory-aligned storage with several key features:
 - Guaranteed 8-byte alignment required by various system calls
 - Zero-cost abstraction for working with raw memory
 - Support for both i8 and u8 types (equivalent for byte operations)
 - Safe access methods with proper bounds checking
 - Lazy initialisation to avoid unnecessary memory writes (initialising with 0's just to overwrite them.)

 # Type Parameters
 - `T`: The element type (i8 or u8)
 - `SIZE`: The fixed capacity of the buffer

 # Safety
 The buffer uses `MaybeUninit` internally, so users must ensure proper
 initialisation before accessing the contents. All unsafe methods document
 their safety requirements.


*/
#[derive(Debug)] // dont derive copy, we want to only hold it in the stack frame and NEVER implicitly copy.
#[repr(C, align(8))] // Ensure 8-byte alignment,uninitialised memory isn't a concern because it's always actually initialised before use.
pub struct AlignedBuffer<T: ValueType, const SIZE: usize>(pub(crate) MaybeUninit<[T; SIZE]>);

impl<T: ValueType, const SIZE: usize> Default for AlignedBuffer<T, SIZE> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ValueType, const SIZE: usize> AlignedBuffer<T, SIZE> {
    /**
    Creates a new uninitialised aligned buffer

    The buffer will have 8-byte alignment but its contents will be uninitialised.
    You must initialised the buffer before accessing its contents.
    */
    #[must_use]
    #[inline]
    #[track_caller]
    pub const fn new() -> Self {
        Self(MaybeUninit::uninit())
    }
    /// Returns the size of which this buffer was created by (counted in raw bytes)
    pub const BUFFER_SIZE: usize = size_of::<T>() * SIZE;

    /// Returns a mutable pointer to the buffer's data
    #[inline]
    #[must_use]
    pub const fn as_mut_ptr(&mut self) -> *mut T {
        (&raw mut self.0).cast()
    }

    /// Returns a const pointer to the buffer's data
    #[inline]
    #[must_use]
    pub const fn as_ptr(&self) -> *const T {
        (&raw const self.0).cast()
    }

    /// Executes the getdents(64) system call using <unistd.h>/direct `libc` syscalls
    /// Supported on Linux/Android/OpenBSD/NetBSD/Solaris/Illumos
    #[inline]
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "solaris",
        target_os = "illumos"
    ))]
    pub fn getdents(&mut self, fd: BorrowedFd) -> isize {
        // SAFETY: we're passing a valid buffer
        unsafe {
            crate::util::getdents64(fd.as_raw_fd(), self.as_mut_ptr().cast(), Self::BUFFER_SIZE)
        }
    }

    /// Executes the `getdirentries64` system call
    /// Supported on macOS and FreeBSD.
    ///
    /// On success, the return value is the number of bytes written into this buffer.
    /// A return value of `0` indicates end-of-directory. Negative values indicate an
    /// OS error as returned by the underlying syscall wrapper.
    ///
    /// Caveats
    /// - This method is `unsafe` because the kernel only initialises the first `n`
    ///   bytes it writes. Callers must only read the returned byte count, not the
    ///   whole buffer.
    /// - `basep` is an in/out directory position cookie. It must point to valid,
    ///   writable memory for the duration of the call, and the updated value should
    ///   be preserved if the caller needs to resume iteration correctly.
    ///
    ///
    ///
    /// # Safety
    /// The caller must ensure `fd` refers to an open directory and that `basep`
    /// remains a valid mutable reference for the duration of the call.
    ///  Only Available on macOS and FreeBSD (with dragonfly to be expected in future update)
    #[inline]
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    pub unsafe fn getdirentries64(&mut self, fd: BorrowedFd, basep: &mut libc::off_t) -> isize {
        // SAFETY: we're passing a valid buffer and valid base pointer
        unsafe {
            crate::util::getdirentries64(
                fd.as_raw_fd(),
                self.as_mut_ptr().cast(),
                Self::BUFFER_SIZE,
                core::ptr::from_mut(basep),
            )
        }
    }

    /**
     Assumes the buffer is initialised and returns a reference to the contents

     # Safety
     The caller must guarantee the entire buffer has been properly initialised
     before calling this method. Accessing uninitialised memory is undefined behavior.
    */
    #[inline]
    pub const unsafe fn assume_init(&self) -> &[T; SIZE] {
        // SAFETY: Caller must ensure the buffer is fully initialised
        unsafe { self.0.assume_init_ref() }
    }

    /**
     Assumes the buffer is initialised and returns a mutable reference to the contents

     # Safety
     The caller must guarantee the entire buffer has been properly initialised
     before calling this method. Accessing uninitialised memory is undefined behavior
    */
    #[inline]
    pub const unsafe fn assume_init_mut(&mut self) -> &mut [T; SIZE] {
        // SAFETY: Caller must ensure the buffer is fully initialised
        unsafe { self.0.assume_init_mut() }
    }
}
