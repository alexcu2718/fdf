#![allow(clippy::cast_possible_wrap)]

use crate::{
    AlignedBuffer, BytePath as _, DirEntry, DirEntryError as Error, LOCAL_PATH_MAX, PathBuffer,
    Result, custom_types_result::BytesStorage, traits_and_conversions::DirentConstructor as _,
};
use core::marker::PhantomData;
use libc::{DIR, closedir, opendir};
#[cfg(not(target_os = "linux"))]
use libc::{dirent as dirent64, readdir};
#[cfg(target_os = "linux")]
use libc::{dirent64, readdir64 as readdir}; //use readdir64 on linux

/// An iterator over directory entries from readdir (or 64 )via libc
/// General POSIX compliant directory iterator.
/// S is a type that implements `BytesStorage`, which is used to store the path bytes.
///
///
// S, which can take forms  Vec<u8>,Box<[u8]>,Arc<[u8]> or ideally SlimmerBytes (an alias in this crate for a smaller box type)
//this is only possible on linux/macos unfortunately.
pub struct DirIter<S>
where
    S: BytesStorage,
{
    pub(crate) dir: *mut DIR,
    pub(crate) path_buffer: PathBuffer,
    pub(crate) file_name_index: u16, //mainly used for indexing tricks, to trivially find the filename(avoid recalculation)
    pub(crate) parent_depth: u16,
    pub(crate) error: Option<Error>,
    pub(crate) _phantom: PhantomData<S>, //this justholds the type information for later, this compiles away due to being zero sized.
}

impl<S> DirIter<S>
where
    S: BytesStorage,
{
    #[inline]
    #[allow(clippy::single_call_fn)]
    //internal function to read the directory entries
    //it is used by the new function to initialise the iterator.
    /// Returns a either
    /// Success => mutpointer to the DIR structure.
    /// Or one of many errors (permissions/etc/ ) that I haven't documented yet. They are handled explicitly however. (essentially my errortype converts from errno)
    pub(crate) fn open_dir(direntry: &DirEntry<S>) -> Result<*mut DIR> {
        let dir = direntry.as_cstr_ptr(|ptr| unsafe { opendir(ptr) });
        // This function reads the directory entries and populates the iterator.
        // It is called when the iterator is created or when it needs to be reset.
        if dir.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }

        Ok(dir)
    }
    #[inline]
    /// Reads the next directory entry from the iterator.
    /// This function reads the directory entries and populates the iterator.
    /// It is called when the iterator is created or when it needs to be reset.
    pub fn get_next_entry(&mut self) -> Option<*const dirent64> {
        let d: *const dirent64 = unsafe { readdir(self.dir) };
        //we have to check for nulls here because we're not 'buffer climbing', aka readdir has abstracted this interface.
        //we do 'buffer climb' (word i just made up) in getdents, which is why this equivalent function does not check the null in my
        //getdents iterator
        if d.is_null() {
            return None;
        }
        Some(d)
    }
    #[inline]
    /// A function to construction a `DirEntry` from the buffer+dirent
    ///
    /// This doesn't need unsafe because the pointer is already checked to not be null before it can be used here.
    pub fn construct_direntry(&mut self, drnt: *const dirent64) -> DirEntry<S> {
        unsafe { self.construct_entry(drnt) }
    }

    #[inline]
    ///now private but explanatory documentation.
    ///Constructs a new `DirIter` from a `DirEntry<S>`.
    /// This function is used to create a new iterator over directory entries.
    /// It takes a `DirEntry<S>` which contains the directory path and other metadata.
    /// It initialises the iterator by opening the directory and preparing the path buffer.
    /// Utilises libc's `opendir` and `readdir64` for directory reading.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::single_call_fn)]
    pub(crate) fn new(dir_path: &DirEntry<S>) -> Result<Self> {
        let dir = Self::open_dir(dir_path)?; //read the directory and get the pointer to the DIR structure.
        let mut path_buffer = AlignedBuffer::<u8, { LOCAL_PATH_MAX }>::new(); //this is a VERY big buffer (filepaths literally cant be longer than this)
        let base_len = unsafe { path_buffer.init_from_direntry(dir_path) };
        //mutate the buffer to contain the full path, then add a null terminator and record the new length
        //we use this length to index to get the filename (store full path -> index to get filename)
        Ok(Self {
            dir,
            path_buffer,
            file_name_index: base_len as _,
            parent_depth: dir_path.depth, //inherit depth
            error: None,                  //set noerrors
            _phantom: PhantomData,        //holds storage type
        })
    }
}

impl<S> core::fmt::Debug for DirIter<S>
where
    S: BytesStorage,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DirIter")
            .field("file_name_index", &self.file_name_index)
            .field("parent_depth", &self.parent_depth)
            .field("error", &self.error)
            .finish_non_exhaustive() //no need to expose anymore than this
    }
}

impl<T> Iterator for DirIter<T>
where
    T: BytesStorage,
{
    type Item = DirEntry<T>;
    #[inline]
    #[allow(clippy::ptr_as_ptr)] //we're aligned so raw pointer as casts are fine.
    fn next(&mut self) -> Option<Self::Item> {
        if self.error.is_some() {
            return None;
        }

        loop {
            let entry = self.get_next_entry()?; //read the next entry from the directory, this is a pointer to the dirent structure.
            //and early return if none

            skip_dot_or_dot_dot_entries!(entry, continue); //we provide the continue here to make it explicit.
            //skip . and .. entries, this macro is a bit evil, makes the code here a lot more concise

            return Some(
                self.construct_direntry(entry), //construct the dirent from the pointer, and the path buffer.
                                                //this is safe because we've already checked if it's null
            );
        }
    }
}
impl<T> Drop for DirIter<T>
where
    T: BytesStorage,
{
    #[inline]
    fn drop(&mut self) {
        if !self.dir.is_null() {
            unsafe { closedir(self.dir) };
        }
    }
}

/*
interesting when testing blk size of via stat calls on my own pc, none had an IO block>4096

// also see reference https://github.com/golang/go/issues/64597, to test this TODO!

libc source code for reference on blk size.
  size_t allocation = default_allocation;
#ifdef _STATBUF_ST_BLKSIZE
  /* Increase allocation if requested, but not if the value appears to
     be bogus.  */
  if (statp != NULL)
    allocation = MIN (MAX ((size_t) statp->st_blksize, default_allocation),
              MAX_DIR_BUFFER_SIZE);
#endif

*/

#[cfg(target_os = "linux")]
///Iterator for directory entries using getdents syscall
pub struct DirEntryIterator<S>
where
    S: BytesStorage,
{
    pub(crate) fd: i32, //fd, this is the file descriptor of the directory we are reading from(it's completely useless after the iterator is dropped)
    pub(crate) buffer: crate::SyscallBuffer, // buffer for the directory entries, this is used to read the directory entries from the  syscall IO, it is 4.1k bytes~ish in size
    pub(crate) path_buffer: crate::PathBuffer, // buffer(stack allocated) for the path, this is used to construct the full path of the entry, this is reused for each entry
    pub(crate) file_name_index: u16, // base path length, this is the length of the path up to and including the last slash (we use these to get filename trivially)
    pub(crate) parent_depth: u16, // depth of the parent directory, this is used to calculate the depth of the child entries
    pub(crate) offset: usize, // offset in the buffer, this is used to keep track of where we are in the buffer
    pub(crate) remaining_bytes: i64, // remaining bytes in the buffer, this is used to keep track of how many bytes are left to read
    pub(crate) _marker: core::marker::PhantomData<S>, // marker for the storage type, this is used to ensure that the iterator can be used with any storage type
                                                      //this gets compiled away anyway as its as a zst
}
#[cfg(target_os = "linux")]
impl<S> Drop for DirEntryIterator<S>
where
    S: BytesStorage,
{
    /// Drops the iterator, closing the file descriptor.
    /// we need to close the file descriptor when the iterator is dropped to avoid resource leaks.
    /// basically you can only have X number of file descriptors open at once, so we need to close them when we are done.
    #[inline]
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) }; //this doesn't return an error code anyway, fuggedaboutit
        //unsafe { close_asm(self.fd) }; //asm implementation, for when i feel like testing if it does anything useful.
    }
}
#[cfg(target_os = "linux")]
impl<S> DirEntryIterator<S>
where
    S: BytesStorage,
{
    #[inline]
    ///Returns a pointer to the `libc::dirent64` in the buffer then increments the offset by the size of the dirent structure.
    /// this is so that when we next time we call `next_getdents_pointer`, we get the next entry in the buffer.
    /// This is unsafe because it dereferences a raw pointer, so we need to ensure that
    /// the pointer is valid and that we don't read past the end of the buffer.
    pub const unsafe fn next_getdents_pointer(&mut self) -> *const libc::dirent64 {
        // This is only used in the iterator implementation, so we can safely assume that the pointer
        // is valid and that we don't read past the end of the buffer.
        let d: *const libc::dirent64 = unsafe { self.buffer.as_ptr().add(self.offset).cast::<_>() };
        self.offset += unsafe { access_dirent!(d, d_reclen) }; //increment the offset by the size of the dirent structure, this is a pointer to the next entry in the buffer
        d //return the pointer
    }
    #[inline]
    /// This is a syscall that fills the buffer (stack allocated) and resets the internal offset counter to 0.
    pub unsafe fn getdents_syscall(&mut self) {
        self.remaining_bytes = unsafe { self.buffer.getdents(self.fd) };
        self.offset = 0;
    }

    #[inline]
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    #[allow(clippy::cast_sign_loss)] //this doesnt matter
    #[allow(clippy::cast_possible_truncation)] //doesnt matter on 64bit
    /// Prefetches the next likely entry in the buffer to keep the cache warm.
    pub(crate) fn prefetch_next_entry(&self) {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if self.offset + 128 < self.remaining_bytes as usize {
                unsafe {
                    use core::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
                    let next_entry = self.buffer.as_ptr().add(self.offset + 64).cast();
                    _mm_prefetch(next_entry, _MM_HINT_T0);
                }
            }
        }
    }
    #[inline]
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)] //not an issue on 64bit
    /// Checks if the buffer is empty
    pub const fn is_buffer_not_empty(&self) -> bool {
        self.offset < self.remaining_bytes as _
    }

    #[inline]
    /// Prefetches the start of the buffer to keep the cache warm.
    pub(crate) fn prefetch_next_buffer(&self) {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            unsafe {
                use core::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
                _mm_prefetch(self.buffer.as_ptr().cast(), _MM_HINT_T0);
            }
        }
    }

    #[inline]
    /// Checks if we're at end of directory
    pub const fn is_end_of_directory(&self) -> bool {
        self.remaining_bytes <= 0
    }
}

#[cfg(target_os = "linux")]
impl<S> Iterator for DirEntryIterator<S>
where
    S: BytesStorage,
{
    type Item = DirEntry<S>;
    #[inline]
    /// Returns the next directory entry in the iterator.
    fn next(&mut self) -> Option<Self::Item> {
        use crate::traits_and_conversions::DirentConstructor as _;
        loop {
            // If we have remaining data in buffer, process it
            if self.is_buffer_not_empty() {
                //we've checked it's not null (albeit, implicitly, so deferencing here is fine.)
                let d: *const libc::dirent64 = unsafe { self.next_getdents_pointer() }; //get next entry in the buffer,
                // this is a pointer to the dirent64 structure, which contains the directory entry information
                self.prefetch_next_entry(); /* check how much is left remaining in buffer, if reasonable to hold more, warm cache this is a no-op on non-x86_64*/

                skip_dot_or_dot_dot_entries!(d, continue); //provide the continue keyword to skip the current iteration if the entry is invalid or a dot entry
                //extract non . and .. files
                let entry = unsafe { self.construct_entry(d) }; //construct the dirent from the pointer, this is a safe function that constructs the DirEntry from the dirent64 structure

                return Some(entry);
            }
            // prefetch the next buffer content before reading

            self.prefetch_next_buffer(); //prefetch the next buffer content to keep the cache warm, this is a no-op on non-x86_64
            // issue a syscall once out of entries
            unsafe { self.getdents_syscall() }; //fill up the buffer again once out  of loop

            if self.is_end_of_directory() {
                // If no more entries, return None,
                return None;
            }
        }
    }
}
