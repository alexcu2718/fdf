#![allow(clippy::must_use_candidate)]
use crate::{
    AlignedBuffer, DirEntry, LOCAL_PATH_MAX, PathBuffer, Result,
    traits_and_conversions::DirentConstructor as _,
};

use core::ptr::NonNull;
//use core::ffi::CStr;
use libc::{DIR, closedir};
#[cfg(not(target_os = "linux"))]
use libc::{dirent as dirent64, readdir};
#[cfg(target_os = "linux")]
use libc::{dirent64, readdir64 as readdir};

//use readdir64 on linux

/// POSIX-compliant directory iterator using libc's readdir functions.
///
/// This iterator traverses directory entries using the standard POSIX directory
/// reading API. It automatically skips "." and ".." entries and provides
/// a safe Rust interface over the underlying C library functions.
///
#[derive(Debug)]
pub struct ReadDir {
    /// Raw directory pointer from libc's `opendir()`
    pub(crate) dir: NonNull<DIR>,
    /// Buffer storing the full directory path for constructing entry paths
    pub(crate) path_buffer: PathBuffer,
    /// Index into `path_buffer` where filenames start (avoids recalculating)
    pub(crate) file_name_index: u16,
    /// Depth of this directory relative to traversal root
    pub(crate) parent_depth: u16,
    /// The file descriptor of this directory, for use in calls like openat/statat etc.
    pub(crate) dirfd: i32,
}

impl ReadDir {
    #[inline]
    //#[expect(clippy::not_unsafe_ptr_arg_deref,reason="The pointer to fd is valid for the duration of the iterator")]
    /// Reads the next directory entry, returning a pointer to it.
    ///
    /// Wraps the libc `readdir` call. Returns `None` when the end of the
    /// directory is reached or an error occurs.
    ///
    pub fn get_next_entry(&mut self) -> Option<*const dirent64> {
        // SAFETY: `self.dir` is a valid directory pointer maintained by the iterator
        let dirent_ptr = unsafe { readdir(self.dir.as_ptr()) };

        // readdir returns null at end of directory or on error
        if dirent_ptr.is_null() {
            None
        } else {
            Some(dirent_ptr)
        }
    }

    /*
    pub fn file_type_from_fd(&self,filename:&CStr)->FileType{
        Fi
    }*/

    #[inline]
    /// Returns the file descriptor for this directory.
    ///
    /// Useful for operations that need the raw directory FD.
    pub const fn dirfd(&self) -> i32 {
        self.dirfd
    }

    #[inline]
    #[expect(
        clippy::not_unsafe_ptr_arg_deref,
        reason = "It is safe to deference while in the lifetime of the iterator"
    )]
    /// A function to construction a `DirEntry` from the buffer+dirent
    pub fn construct_direntry(&mut self, drnt: *const dirent64) -> DirEntry {
        // SAFETY:  This doesn't need unsafe because the pointer is already checked to not be null before it can be used here.
        unsafe { self.construct_entry(drnt) }
    }

    #[inline]
    ///now private but explanatory documentation.
    /// This function is used to create a new iterator over directory entries.
    /// It takes a `DirEntry<S>` which contains the directory path and other metadata.
    /// It initialises the iterator by opening the directory and preparing the path buffer.
    /// Utilises libc's `opendir` and `readdir64` for directory reading.
    pub(crate) fn new(dir_path: &DirEntry) -> Result<Self> {
        // SAFETY: We are passing a null terminated string.
        let dir = unsafe { dir_path.open_dir()? }; //read the directory and get the pointer to the DIR structure.
        let mut path_buffer = AlignedBuffer::<u8, { LOCAL_PATH_MAX }>::new(); //this is a VERY big buffer (filepaths literally cant be longer than this)
        // SAFETY:This pointer is forcefully null terminated and below PATH_MAX (system dependent)
        let base_len = unsafe { path_buffer.init_from_direntry(dir_path) };
        //mutate the buffer to contain the full path, then add a null terminator and record the new length
        //we use this length to index to get the filename (store full path -> index to get filename)
        // SAFETY:This is a valid pointer from a just opened directory
        let dirfd = unsafe { libc::dirfd(dir.as_ptr()) };
        Ok(Self {
            dir,
            path_buffer,
            file_name_index: base_len as _,
            parent_depth: dir_path.depth, //inherit depth
            dirfd,
        })
    }
}

impl Iterator for ReadDir {
    type Item = DirEntry;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = self.get_next_entry()?; //read the next entry from the directory, this is a pointer to the dirent structure.
            //and early return if none
            // SAFETY: we know the pointer is not null therefor the operations in this macro are fine to use.
            skip_dot_or_dot_dot_entries!(entry, continue); //we provide the continue here to make it explicit.
            //skip . and .. entries, this macro is a bit evil, makes the code here a lot more concise

            return Some(
                self.construct_direntry(entry), //construct the dirent from the pointer, and the path buffer.
                                                //this is safe because we've already checked if it's null
            );
        }
    }
}
impl Drop for ReadDir {
    #[inline]
    /// Closes the directory file descriptor to prevent resource leaks.
    ///
    /// File descriptors are limited system resources, so proper cleanup
    /// is essential.
    fn drop(&mut self) {
        // SAFETY: we've know it's not null and we need to close it to prevent the fd staying open
        unsafe { closedir(self.dir.as_ptr()) };
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
/// Linux-specific directory iterator using the `getdents` syscall.
///
/// More efficient than `readdir` for large directories due to batched reads.
/// Doesn't implicitly call stat unless on unusual filesystems.
pub struct GetDents {
    pub(crate) fd: i32,
    /// File descriptor of the open directory
    pub(crate) buffer: crate::SyscallBuffer, // buffer for the directory entries, this is used to read the directory entries from the  syscall IO, it is 4.1k bytes~ish in size
    pub(crate) path_buffer: crate::PathBuffer, // buffer(stack allocated) for the path, this is used to construct the full path of the entry, this is reused for each entry
    pub(crate) file_name_index: u16, // base path length, this is the length of the path up to and including the last slash (we use these to get filename trivially)
    pub(crate) parent_depth: u16, // depth of the parent directory, this is used to calculate the depth of the child entries
    pub(crate) offset: usize, // offset in the buffer, this is used to keep track of where we are in the buffer
    pub(crate) remaining_bytes: i64, // remaining bytes in the buffer, this is used to keep track of how many bytes are left to read
                                     //this gets compiled away anyway as its as a zst
}
#[cfg(target_os = "linux")]
impl Drop for GetDents {
    /// Drops the iterator, closing the file descriptor.
    /// we need to close the file descriptor when the iterator is dropped to avoid resource leaks.
    /// basically you can only have X number of file descriptors open at once, so we need to close them when we are done.
    #[inline]
    fn drop(&mut self) {
        // SAFETY: we've know the fd is valid and we're closing it as our drop impl
        unsafe { libc::close(self.fd) }; //this doesn't return an error code anyway, fuggedaboutit
        //unsafe { crate::syscalls::close_asm(self.fd) }; //asm implementation, for when i feel like testing if it does anything useful.
    }
}

#[cfg(target_os = "linux")]
impl GetDents {
    #[inline]
    /// Advances to the next directory entry in the buffer and returns a pointer to it.
    ///
    /// Increments the internal offset by the entry's record length, positioning the iterator
    /// at the next entry for subsequent calls.
    ///
    /// # Safety
    /// - The buffer must contain valid `dirent64` structures
    /// - `self.offset` must point to a valid entry within the buffer bounds
    /// - The caller must ensure we don't read past the end of the buffer
    pub const unsafe fn get_next_entry(&mut self) -> *const libc::dirent64 {
        // SAFETY: This is only used in the iterator implementation, so we can safely assume that the pointer
        // is valid and that we don't read past the end of the buffer.
        let d: *const libc::dirent64 = unsafe { self.buffer.as_ptr().add(self.offset).cast::<_>() };
        // SAFETY: we've checked it's not null
        self.offset += unsafe { access_dirent!(d, d_reclen) }; //increment the offset by the size of the dirent structure, this is a pointer to the next entry in the buffer
        d //return the pointer
    }

    #[inline]
    /// Fills the buffer with directory entries using the getdents system call.
    ///
    /// Returns `true` if new entries were read, `false` if end of directory.
    ///
    /// # Safety
    /// - File descriptor must be valid and open
    pub unsafe fn fill_buffer(&mut self) -> bool {
        // SAFETY: This is a valid fd (its open for the lifetime of the iterator)
        self.remaining_bytes = unsafe { self.buffer.getdents(self.fd) };
        self.offset = 0;
        self.remaining_bytes > 0 //if remaining_bytes<0 then we've reached the end.
    }

    #[inline]
    /**
    Returns the file descriptor for this directory.

    Useful for operations that need the raw directory FD.
    */
    pub const fn dirfd(&self) -> i32 {
        self.fd
    }

    #[inline]
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::undocumented_unsafe_blocks)] //comment these later TODO!
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
    pub(crate) fn new(dir: &DirEntry) -> Result<Self> {
        use crate::SyscallBuffer;
        // SAFETY: We're  null terminating the filepath and it's below `LOCAL_PATH_MAX` (4096/1024 system dependent)
        let fd = unsafe { dir.open_fd()? }; //returns none if null (END OF DIRECTORY/Directory no longer exists) (we've already checked if it's a directory/symlink originally )
        let mut path_buffer = AlignedBuffer::<u8, { LOCAL_PATH_MAX }>::new(); //nulll initialised  (stack) buffer that can axiomatically hold any filepath.
        // SAFETY: The filepath provided is axiomatically less than size `LOCAL_PATH_MAX`
        let path_len = unsafe { path_buffer.init_from_direntry(dir) };
        //TODO! make this more ergonomic
        let buffer = SyscallBuffer::new();
        Ok(Self {
            fd,
            buffer,
            path_buffer,
            file_name_index: path_len as _,
            parent_depth: dir.depth,
            offset: 0,
            remaining_bytes: 0,
        })
    }

    #[inline]
    #[allow(clippy::cast_sign_loss)]
    /// Checks if the buffer is empty
    pub const fn is_buffer_not_empty(&self) -> bool {
        self.offset < self.remaining_bytes as _
    }

    #[inline]
    #[allow(clippy::undocumented_unsafe_blocks)] //comment these later TODO!
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
}

#[cfg(target_os = "linux")]
impl Iterator for GetDents {
    type Item = DirEntry;
    #[inline]
    /// Returns the next directory entry in the iterator.
    fn next(&mut self) -> Option<Self::Item> {
        use crate::traits_and_conversions::DirentConstructor as _;
        loop {
            // If we have remaining data in buffer, process it
            if self.is_buffer_not_empty() {
                // SAFETY: we've checked it's not null (albeit, implicitly, so deferencing here is fine.)
                let d: *const libc::dirent64 = unsafe { self.get_next_entry() }; //get next entry in the buffer,
                // this is a pointer to the dirent64 structure, which contains the directory entry information
                self.prefetch_next_entry(); /* check how much is left remaining in buffer, if reasonable to hold more, warm cache this is a no-op on non-x86_64*/
                // SAFETY: we know the pointer is not null therefor the operations in this macro are fine to use.
                skip_dot_or_dot_dot_entries!(d, continue); //provide the continue keyword to skip the current iteration if the entry is invalid or a dot entry
                //extract non . and .. files
                // SAFETY: As the above safety comment states
                //construct the dirent from the pointer, this is a safe function that constructs the DirEntry from the dirent64 structure
                return Some(unsafe { self.construct_entry(d) });
            }
            // prefetch the next buffer content before reading

            self.prefetch_next_buffer(); //prefetch the next buffer content to keep the cache warm, this is a no-op on non-x86_64
            // issue a syscall once out of entries
            // SAFETY: the file descriptor is still open and is valid to call
            if unsafe { self.fill_buffer() } {
                continue; // New entries available, restart loop
            }

            return None; //signal end of directory
        }
    }
}
