use std::mem::MaybeUninit;
use std::ops::{Index, IndexMut};
use std::slice::SliceIndex;
mod sealed {
    pub trait Sealed {}
    impl Sealed for i8 {}
    impl Sealed for u8 {}
}

pub trait ValueType: sealed::Sealed {}
impl ValueType for i8 {}
impl ValueType for u8 {}

// This buffer is in this crate to do a few things:
//1.serve as a generic buffer for syscall operations
//2.ensure that the buffer is always aligned to 8 bytes, which is required for some syscalls.
//3. provide a safe interface for accessing the buffer's data.
//4. It is generic over type T, which can be either i8 or u8, and the size of the buffer. which are equivalent in our case.
//5. It uses MaybeUninit to avoid initialising the buffer until it is actually used, which is useful for performance.
//6. it provides a buffer to construct byte path
#[derive(Debug)]
#[repr(C, align(8))] // Ensure 8-byte alignment,uninitialised memory isn't a concern because it's always actually initialised before use.
pub struct AlignedBuffer<T, const SIZE: usize>
where
    T: ValueType, //only generic over i8 and u8!
{
    //generic over size.
    data: MaybeUninit<[T; SIZE]>,
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
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            data: MaybeUninit::uninit(),
        }
    }

    #[inline]
    #[must_use]
    pub const fn as_mut_ptr(&mut self) -> *mut T {
        self.data.as_mut_ptr().cast()
    }

    #[inline]
    #[must_use]
    #[allow(dead_code)] //not used YET
    pub const fn as_ptr(&self) -> *const T {
        self.data.as_ptr().cast()
    }

    /// # Safety
    /// The buffer must be initialised before calling this
    #[inline]
    pub const unsafe fn as_slice(&self) -> &[T] {
        unsafe { &*self.data.as_ptr() }
    }

    /// # Safety
    /// The buffer must be initialised before calling this
    #[inline]
    #[allow(dead_code)]
    pub const unsafe fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { &mut *self.data.as_mut_ptr() }
    }

    /// # Safety
    /// this is only to be called when using syscalls in the getdents interface
    #[inline]
    #[cfg(target_os = "linux")]
    pub(crate) unsafe fn getdents64_internal(&mut self, fd: i32) -> i64 {
        unsafe { Self::getdents64_asm(self,fd) }
    }

    /// # Safety
    /// this is only to be called when using syscalls in the getdents interface
    /// This uses inline assembly, in theory it should be equivalent but glibc is 'quirky'.
    /// At the end of the day, the only way to bypass glibc's quirks is to use inline assembly.
    ///
    /// TODO! write an implementation for RISC-V
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    #[cfg(all(target_os = "linux"))]
    #[allow(dead_code)]
    pub(crate) unsafe fn getdents64_asm(&mut self, fd: i32) -> i64 {
        unsafe { crate::syscalls::getdents_asm(fd, self.as_mut_ptr(), SIZE) }
    }

    /// # Safety
    /// The range must be within initialised portion of the buffer
    #[inline]
    pub unsafe fn get_unchecked<R>(&self, range: R) -> &R::Output
    where
        R: SliceIndex<[T]>,
    {
        unsafe { self.as_slice().get_unchecked(range) }
    }

    #[inline]
    /// Initialises the buffer with the given path, appending a slash if needed.
    ///
    /// Returns the new base length after writing into the buffer.
    ///
    /// # Safety
    /// Assumes `self` is zeroed and sized at least `LOCAL_PATH_MAX`.
    pub(crate) unsafe fn init_from_direntry<S>(&mut self, dir_path: &crate::DirEntry<S>) -> usize
    where
        S: crate::BytesStorage,
    {
        let buffer_ptr = self.as_mut_ptr(); // get the mutable pointer to the buffer

        let mut base_len = dir_path.len(); // get length of directory path
        let needs_slash = (dir_path.as_bytes() != b"/") as u8; // check if we need to append a slash

        unsafe {
            std::ptr::copy_nonoverlapping(dir_path.as_ptr(), buffer_ptr.cast(), base_len); // copy path
            *buffer_ptr.cast::<u8>().add(base_len) = b'/' * needs_slash; // add slash if needed
        } //cast into byte types

        base_len += needs_slash as usize; // update length if slash added

        base_len
    }

    /// # Safety
    /// The range must be within initialised portion of the buffer
    #[inline]
    #[allow(dead_code)]
    pub unsafe fn get_unchecked_mut<R>(&mut self, range: R) -> &mut R::Output
    where
        R: SliceIndex<[T]>,
    {
        unsafe { self.as_mut_slice().get_unchecked_mut(range) }
    }

    /// # Safety
    /// The entire buffer must be initialised
    #[inline]
    const unsafe fn assume_init(&self) -> &[T; SIZE] {
        unsafe { &*self.data.as_ptr() }
    }

    /// # Safety
    /// The entire buffer must be initialised
    #[inline]
    const unsafe fn assume_init_mut(&mut self) -> &mut [T; SIZE] {
        unsafe { &mut *self.data.as_mut_ptr() }
    }
}
