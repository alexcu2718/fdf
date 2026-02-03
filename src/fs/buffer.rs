#![allow(clippy::multiple_unsafe_ops_per_block)] //annoying convention

use core::marker::Copy;
use core::mem::MaybeUninit;
use core::ops::{Index, IndexMut};
use core::slice::SliceIndex;
mod sealed {
    /// Sealed trait pattern to restrict `ValueType` implementation to i8 and u8 only
    pub trait Sealed {}
    impl Sealed for i8 {}
    impl Sealed for u8 {}
}

/// Marker trait for valid buffer value types (i8 and u8)
///
/// This trait ensures type safety while allowing the buffer to work with both
/// signed and unsigned byte types, which are equivalent for raw memory operations.
pub trait ValueType: sealed::Sealed + Copy {}

impl ValueType for i8 {}
impl ValueType for u8 {}

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

 # Examples
 ```
 use fdf::fs::AlignedBuffer;

 // Create a new aligned buffer
 // Purposely set a non-aligned amount to show alignment is forced.
 let mut buffer = AlignedBuffer::<u8, 1026>::new(); //You should really use 1024 here.

 // Initialise the buffer with data
 let data = b"Hello, World!";
 unsafe {
     // Copy data into the buffer
     core::ptr::copy_nonoverlapping(
         data.as_ptr(),
         buffer.as_mut_ptr(),
         data.len()
     );

     // Access the initialised data
     let slice = buffer.get_unchecked(0..data.len());
     assert_eq!(slice, data);

     // Modify the buffer contents
     let mut_slice = buffer.get_unchecked_mut(0..data.len());
     mut_slice[0] = b'h'; // Change 'H' to 'h'
     assert_eq!(&mut_slice[0..5], b"hello");
 }

 // The buffer maintains proper alignment for syscalls
 //Protip: NEVER cast a ptr to a usize unless you're extremely sure of what you're doing!
 assert!((buffer.as_ptr() as usize) % 8 ==0,"We expect the buffer to be aligned to 8 bytes")
 // assert!((buffer.as_ptr() as usize).is_multiple_of(8),"We expect the buffer to be aligned to 8 bytes")
 ```
*/
#[derive(Debug)]
#[repr(C, align(8))] // Ensure 8-byte alignment,uninitialised memory isn't a concern because it's always actually initialised before use.
pub struct AlignedBuffer<T, const SIZE: usize>
where
    T: ValueType, // Only generic over i8 and u8!
{
    // Generic over size.
    pub(crate) data: MaybeUninit<[T; SIZE]>,
}

impl<T, const SIZE: usize, Idx> Index<Idx> for AlignedBuffer<T, SIZE>
where
    T: ValueType,
    Idx: SliceIndex<[T]>,
{
    type Output = Idx::Output;

    #[inline]
    fn index(&self, index: Idx) -> &Self::Output {
        // SAFETY: The buffer must initialised
        unsafe { self.assume_init().get_unchecked(index) }
    }
}

impl<T, const SIZE: usize, Idx> IndexMut<Idx> for AlignedBuffer<T, SIZE>
where
    T: ValueType,
    Idx: SliceIndex<[T]>,
{
    #[inline]
    fn index_mut(&mut self, index: Idx) -> &mut Self::Output {
        // SAFETY: The buffer must be initialised before access
        unsafe { self.assume_init_mut().get_unchecked_mut(index) }
    }
}
#[allow(clippy::new_without_default)]
impl<T, const SIZE: usize> AlignedBuffer<T, SIZE>
where
    T: ValueType,
{
    /**
    Creates a new uninitialised aligned buffer

    The buffer will have 8-byte alignment but its contents will be uninitialised.
    You must initialised the buffer before accessing its contents.
    */
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self {
            data: MaybeUninit::uninit(),
        }
    }

    /// Returns a mutable pointer to the buffer's data
    #[inline]
    #[must_use]
    pub const fn as_mut_ptr(&mut self) -> *mut T {
        self.data.as_mut_ptr().cast()
    }

    /// Returns a const pointer to the buffer's data
    #[inline]
    #[must_use]
    pub const fn as_ptr(&self) -> *const T {
        self.data.as_ptr().cast()
    }

    /**
     Returns a slice of the buffer's contents

     # Safety
     The buffer must be fully initialised before calling this method.
     Accessing uninitialised memory is undefined behavior.
    */
    #[inline]
    pub const unsafe fn as_slice(&self) -> &[T] {
        // SAFETY: Caller must ensure the buffer is fully initialised
        unsafe { &*self.data.as_ptr() }
    }
    /**
     Returns a mutable slice of the buffer's contents

     # Safety
     The buffer must be fully initialised before calling this method.
     Accessing uninitialised memory is undefined behavior.
    */
    #[inline]
    pub const unsafe fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY: Caller must ensure the buffer is fully initialised
        unsafe { &mut *self.data.as_mut_ptr() }
    }

    /// Executes the getdents64 system call using <unistd.h>/direct `libc` syscalls
    #[inline]
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub fn getdents(&mut self, fd: &crate::fs::FileDes) -> i64 {
        // SAFETY: we're passing a valid buffer
        unsafe { crate::util::getdents(fd.0, self.as_mut_ptr(), SIZE) }
    }

    #[inline]
    #[cfg(target_os = "macos")]
    #[allow(clippy::not_unsafe_ptr_arg_deref)] //shutup
    pub fn getdirentries(&mut self, fd: &crate::fs::FileDes, basep: *mut i64) -> i32 {
        // SAFETY: we're passing a valid buffer
        unsafe { crate::util::getdirentries64(fd.0, self.as_mut_ptr(), SIZE, basep) }
    }

    // TODO: Maybe delete this?
    // #[inline] Irrelevant because of macos semantics.
    // #[cfg(target_os = "macos")]
    // #[allow(clippy::not_unsafe_ptr_arg_deref)] // Shut up
    // pub fn getdirentries(&mut self, fd: &crate::FileDes, basep: *mut i64) -> i32 {
    //     // SAFETY: we're passing a valid buffer
    //     unsafe { crate::utils::getdirentries64(fd.0, self.as_mut_ptr(), SIZE, basep) }
    // }

    /**
     Returns a reference to a subslice without doing bounds checking

     # Safety

     The caller must ensure:
     - The buffer is fully initialised
     - The range is within the bounds of the buffer (0..SIZE)
     - The range does not access uninitialised memory
    */
    #[inline]
    pub unsafe fn get_unchecked<R>(&self, range: R) -> &R::Output
    where
        R: SliceIndex<[T]>,
    {
        // SAFETY: Caller must ensure the buffer is initialised and range is valid
        unsafe { self.as_slice().get_unchecked(range) }
    }

    /**
    Returns a mutable reference to a subslice without doing bounds checking

    # Safety
    The range must be within initialised portion of the buffer
    */
    #[inline]
    pub unsafe fn get_unchecked_mut<R>(&mut self, range: R) -> &mut R::Output
    where
        R: SliceIndex<[T]>,
    {
        // SAFETY: Caller must ensure the buffer is fully initialised
        unsafe { self.as_mut_slice().get_unchecked_mut(range) }
    }

    /**
     Assumes the buffer is initialised and returns a reference to the contents

     # Safety
     The caller must guarantee the entire buffer has been properly initialised
     before calling this method. Accessing uninitialised memory is undefined behavior.
    */
    #[inline]
    const unsafe fn assume_init(&self) -> &[T; SIZE] {
        // SAFETY: Caller must ensure the buffer is fully initialised
        unsafe { &*self.data.as_ptr() }
    }

    /**
     Assumes the buffer is initialised and returns a mutable reference to the contents

     # Safety
     The caller must guarantee the entire buffer has been properly initialised
     before calling this method. Accessing uninitialised memory is undefined behavior
    */
    #[inline]
    const unsafe fn assume_init_mut(&mut self) -> &mut [T; SIZE] {
        // SAFETY: Caller must ensure the buffer is fully initialised
        unsafe { &mut *self.data.as_mut_ptr() }
    }
}
