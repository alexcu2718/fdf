#![allow(clippy::must_use_candidate)]
#[cfg(target_os = "linux")]
use crate::SyscallBuffer;
use crate::{
    DirEntry, FileDes, PathBuffer, Result, traits_and_conversions::DirentConstructor as _,
};

use core::ptr::NonNull;
use libc::DIR;
#[cfg(not(target_os = "linux"))]
use libc::{dirent as dirent64, readdir};
#[cfg(target_os = "linux")]
use libc::{dirent64, readdir64 as readdir};

/**
 POSIX-compliant directory iterator using libc's readdir functions.

 This iterator traverses directory entries using the standard POSIX directory
 reading API. It automatically skips "." and ".." entries and provides
 a safe Rust interface over the underlying C library functions.

*/
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
    pub(crate) dirfd: FileDes,
}

impl ReadDir {
    #[inline]
    /**
    Reads the next directory entry, returning a pointer to it.

    Wraps the libc `readdir` call.

    Returns `None` when the end of the directory is reached or an error occurs.

     */
    pub fn get_next_entry(&mut self) -> Option<NonNull<dirent64>> {
        // SAFETY: `self.dir` is a valid directory pointer maintained by the iterator
        let dirent_ptr = unsafe { readdir(self.dir.as_ptr()) };

        // readdir returns null at end of directory or on error
        NonNull::new(dirent_ptr)
    }

    #[inline]
    /**
      Returns the file descriptor for this directory.

      Useful for operations that need the raw
    */
    pub const fn dirfd(&self) -> &FileDes {
        &self.dirfd
    }

    #[inline]
    /**
     Constructs a `DirEntry` from a directory entry pointer.

     This method converts a raw `dirent64` pointer into a safe `DirEntry`
     by combining the directory entry metadata with the parent directory's
     path information stored in the path buffer.

     # Arguments
     * `drnt` - Non-null pointer to a valid `dirent64` structure

    */
    pub fn construct_direntry(&mut self, drnt: NonNull<dirent64>) -> DirEntry {
        // SAFETY:  Because the pointer is already checked to not be null before it can be used here safely
        unsafe { self.construct_entry(drnt.as_ptr()) }
    }

    #[inline]
    pub(crate) fn new(dir_path: &DirEntry) -> Result<Self> {
        let dir_stream = dir_path.open_dir()?; //read the directory and get the pointer to the DIR structure.
        // SAFETY:This pointer is forcefully null terminated and below PATH_MAX (system dependent)
        let (path_buffer, path_len) = unsafe { PathBuffer::init_from_direntry(dir_path) };
        //mutate the buffer to contain the full path, then add a null terminator and record the new length
        //we use this length to index to get the filename (store full path -> index to get filename)

        // SAFETY:   dir is a non null pointer,the pointer is guaranteed to be valid
        let dirfd = unsafe { FileDes(libc::dirfd(dir_stream.as_ptr())) };
        debug_assert!(dirfd.is_open(), "We expect it to be open");

        Ok(Self {
            dir: dir_stream,
            path_buffer,
            file_name_index: path_len,
            parent_depth: dir_path.depth, //inherit depth
            dirfd,
        })
    }
}

/*
   This operations is essentially just a struct field access cost(no syscall/blocking io), the pointer is guaranteed to be valid because
 I found reading into this interesting, never heard of opaque pointers in C before this, i assumed C was public everything,
 see this below

                struct __dirstream
{
    off_t tell;
    int fd;
    int buf_pos;
    int buf_end;
    volatile int lock[1];
    /* Any changes to this struct must preserve the property:
     * offsetof(struct __dirent, buf) % sizeof(off_t) == 0 */
    char buf[2048];
};

         */

impl Iterator for ReadDir {
    type Item = DirEntry;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = self.get_next_entry()?; //read the next entry from the directory, this is a pointer to the dirent structure.
            //and early return if none
            // SAFETY: we know the pointer is not null therefor the operations in this macro are fine to use.
            skip_dot_or_dot_dot_entries!(entry.as_ptr(), continue); //we provide the continue here to make it explicit.
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
        debug_assert!(
            self.dirfd.is_open(),
            "We expect the file descriptor to be open before closing"
        );
        self.dirfd.close_fd()
        //unsafe { crate::syscalls::close_asm(self.fd.0) }; //asm implementation, for when i feel like testing if it does anything useful.
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
/**
 Linux-specific directory iterator using the `getdents` system call.

 Provides more efficient directory traversal than `readdir` for large directories
 by performing batched reads into a kernel buffer. This reduces system call overhead
 and improves performance when scanning directories with many entries.

 Unlike some directory iteration methods, this does not implicitly call `stat`
 on each entry unless required by unusual filesystem behaviour.
*/
pub struct GetDents {
    /// File descriptor of the open directory, wrapped for automatic resource management
    pub(crate) fd: FileDes,
    /// Kernel buffer for batch reading directory entries via system call I/O
    /// Approximately 4.1KB in size, optimised for typical directory traversal
    pub(crate) buffer: SyscallBuffer,
    /// Stack-allocated buffer for constructing full entry paths
    /// Reused for each entry to avoid repeated memory allocation
    pub(crate) path_buffer: PathBuffer,
    /// Length of the base directory path including the trailing slash
    /// Used for efficient filename extraction and path construction
    pub(crate) file_name_index: u16,
    /// Depth of the parent directory in the directory tree hierarchy
    /// Used to calculate depth for child entries during recursive traversal
    pub(crate) parent_depth: u16,
    /// Current read position within the directory entry buffer
    /// Tracks progress through the currently loaded batch of entries
    pub(crate) offset: usize,
    /// Number of bytes remaining to be processed in the current buffer
    /// Indicates when a new system call is needed to fetch more entries
    pub(crate) remaining_bytes: usize,
    /// A marker for when the `FileDes` can give no more entries
    pub(crate) end_of_stream: bool,
}
#[cfg(target_os = "linux")]
impl Drop for GetDents {
    /**
      Drops the iterator, closing the file descriptor.
      we need to close the file descriptor when the iterator is dropped to avoid resource leaks.

      basically you can only have X number of file descriptors open at once, so we need to close them when we are done.
    */
    #[inline]
    fn drop(&mut self) {
        debug_assert!(
            self.fd.is_open(),
            "We expect the file descriptor to be open before closing"
        );
        self.fd.close_fd()
        //unsafe { crate::syscalls::close_asm(self.fd.0) }; //asm implementation, for when i feel like testing if it does anything useful.
    }
}

#[cfg(target_os = "linux")]
impl GetDents {
    #[inline]
    /**
      Advances to the next directory entry in the buffer and returns a pointer to it.

      Increments the internal offset by the entry's record length, positioning the iterator
      at the next entry for subsequent calls.

      # Safety
      - The buffer must contain valid `dirent64` structures
      - You must check if `is_buffer_not_empty` is true before calling.
    */
    pub const unsafe fn get_next_entry(&mut self) -> NonNull<dirent64> {
        // SAFETY: the buffer must contain enough (checked by caller).
        let d: *const libc::dirent64 = unsafe { self.buffer.as_ptr().add(self.offset).cast::<_>() };
        // SAFETY: By precondition
        self.offset += unsafe { access_dirent!(d, d_reclen) }; //increment the offset by the size of the dirent structure, this is a pointer to the next entry in the buffer
        // SAFETY: as above
        unsafe { NonNull::new_unchecked(d.cast_mut()) } //return the pointer
    }
    #[inline]
    #[must_use]
    /// Returns whether the directory stream has reached its end.
    ///
    /// This method indicates that no more directory entries can be read from this stream.
    /// Once `true`, subsequent calls to [`fill_buffer()`](Self::fill_buffer) will return `false`
    /// and the iterator will return `None`.
    ///
    /// # Returns
    /// - `true` if the directory stream has ended (EOF reached or buffer capacity insufficient)
    /// - `false` if more directory entries may be available to read
    ///
    /// # Examples
    /// ```
    /// use fdf::DirEntry;
    /// let dir = DirEntry::new("/tmp").unwrap();
    /// let mut getdents = dir.getdents().unwrap();
    ///
    /// // Process entries until end of stream
    /// while !getdents.is_end_of_stream() {
    ///     if getdents.fill_buffer() {
    ///         // Process entries in buffer...
    ///     }
    /// }
    /// println!("Reached end of directory");
    /// ```
    pub const fn is_end_of_stream(&self) -> bool {
        self.end_of_stream
    }

    #[inline]
    /**
      Constructs a `DirEntry` from a directory entry pointer.

      This method converts a raw `dirent64` pointer into a safe `DirEntry`
      by combining the directory entry metadata with path information from
      the internal path buffer. The resulting `DirEntry` contains the full
      path and metadata for filesystem traversal.

      # Arguments
      * `drnt` - Pointer to a valid `dirent64` structure from the getdents buffer
    */
    pub fn construct_direntry(&mut self, drnt: NonNull<dirent64>) -> DirEntry {
        // SAFETY:  Because the pointer is already checked to not be null before it can be used here.
        unsafe { self.construct_entry(drnt.as_ptr()) }
    }

    #[inline]
    /**
     Returns the number of unprocessed bytes remaining in the current kernel buffer.

     This indicates how much data is still available to be processed before needing
     to perform another `getdents64` system call. When this returns 0, the buffer
     has been exhausted and [`read_more_entries`] should be called to fetch the
     next batch of directory entries from the kernel.

     # Examples
     ```
      use fdf::DirEntry;
     let start_path=std::env::temp_dir();
       let getdents=DirEntry::new(start_path).unwrap().getdents().unwrap();
     while getdents.remaining_bytes() > 0 {
         // Process entries from current buffer
     }
     // Buffer exhausted, need to read more
     ```

    */
    pub const fn remaining_bytes(&self) -> usize {
        self.remaining_bytes
    }

    #[inline]
    #[allow(clippy::integer_division)] // Trust me, you dont want floats.
    #[allow(clippy::integer_division_remainder_used)]
    /**

     This provides a lower bound by dividing the remaining bytes by the size of a `dirent64`
     structure. Since actual directory entries have variable sizes (due to different filename
     lengths), this represents the worst-case scenario where all remaining entries use the
     maximum possible space.

     # Note
     The actual number of entries WILL ALWAYS be higher than this estimate because:
     - Directory entries with shorter names consume less space
     - In theory, a buffer can hold up to 11 (extremely short name) files,
     - EG: 11*24 (minimum reclen/size of `dirent64` struct)==264, while `size_of` a `dirent64` will always be 280
     # Examples
     ```
     use fdf::DirEntry;
     let start_path=std::env::temp_dir();
     let getdents=DirEntry::new(start_path).unwrap().getdents().unwrap();
     // Use for pre-allocation or progress estimation
     let min_entries = getdents.minimum_direntries_left();
     if min_entries > 100 {
         // Likely many entries remaining
     }
     ```
    */
    pub const fn minimum_direntries_left(&self) -> usize {
        self.remaining_bytes / size_of::<dirent64>() // floor division = minimum
        //self.remaining_bytes.div_floor(size_of::<dirent64>())
        // use this when const num traits is stable FIXME
    }

    /**
    Returns the absolute maximum number of directory entries that could theoretically fit
    in the entire buffer, assuming all entries use the minimum possible size.

       */
    pub const fn max_direntries_possible(&self) -> usize {
        self.buffer.max_capacity().div_ceil(24)
    }

    #[inline]
    #[allow(clippy::integer_division)]
    /**
     This provides an upper bound by dividing the remaining bytes by the minimum possible entry size.
     Since directory entries have variable sizes but cannot be smaller than the fixed header portion
     plus at least 1 byte for the filename, this represents the best-case scenario where all
     remaining entries use the absolute minimum space.

     # Note
     The actual number of entries WILL ALWAYS be less than or equal to this estimate because:
     - No directory entry can be smaller than the fixed header overhead
     - On Linux `x86_64`, the minimum `d_reclen` is typically 24 bytes (header + null terminator)
     - This gives the theoretical maximum possible entries that could fit in the remaining buffer

     # Examples
     ```
     use fdf::DirEntry;
     let start_path = std::env::temp_dir();
     let getdents = DirEntry::new(start_path).unwrap().getdents().unwrap();
     // Use for worst-case memory allocation
     let max_entries = getdents.maximum_direntries_left();
     if max_entries < 1000 {
         // We have a reasonable upper bound
     }
     ```
    */
    #[allow(clippy::integer_division_remainder_used)]
    pub const fn maximum_direntries_left(&self) -> usize {
        //self.remaining_bytes.div_floor(24)
        // use this when const num traits is stable FIXME
        self.remaining_bytes / 24
    }

    #[inline]
    /**
     Fills the buffer with directory entries using the getdents system call.

     This method attempts to read more directory entries into the internal buffer.
     If the directory stream has already ended, returns immediately.

     # Smart End-of-Stream Detection

     This method uses an optimisation to minimise system calls by detecting when
     the directory has been exhausted before getdents returns 0 bytes:

     - A full directory read returns exactly `buffer.max_capacity()` bytes
     - A partial read (end approaching) returns fewer bytes
     - If returned bytes ≤ `(max_capacity - size_of::<dirent64>())`, there cannot be
       another full buffer's worth of data remaining, indicating EOF

     This avoids making an extra system call that would return 0 bytes.

     # Returns

     - `true` if new directory entries were read into the buffer
     - `false` if:
       - The directory stream has already ended (`is_end_of_stream()` was `true`)
       - No entries were read (0 bytes returned)
       - The optimisation detected EOF (partial buffer read with insufficient remaining data)

     # State Transitions

     - Sets `end_of_stream = true` when EOF is detected via zero bytes or the optimisation
     - Resets `offset = 0` to start reading from the beginning of new buffer data
     - Updates `remaining_bytes` with the actual bytes read from the system call
    */
    #[expect(
        clippy::cast_sign_loss,
        reason = "Negative error codes from getdents are explicitly handled by max(0)"
    )]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "i64 to usize truncation is acceptable as buffer sizes are limited"
    )]
    pub fn fill_buffer(&mut self) -> bool {
        // Early return if we've already reached end of stream
        if self.is_end_of_stream() {
            return false;
        }

        // Read directory entries, ignoring negative error codes
        self.remaining_bytes = (self.buffer.getdents(&self.fd).max(0)) as usize;
        let no_bytes_left = self.remaining_bytes == 0;
        /*

         Smart end-of-stream detection: Avoid unnecessary system calls by detecting when
         we've likely exhausted the directory based on the returned byte count.

         Why this works:
         - A full directory read returns exactly buffer.max_capacity() bytes
         - A partial read (end approaching) returns less than maximum
         - If returned bytes ≤ (max_capacity - largest_dirent_size), there can't be
           another full buffer's worth of data remaining

         Example:
         - Buffer capacity: 4600 bytes (It is arbitrary)
         - Largest dirent64 size: 280 bytes
         - If getdents returns ≤ 4320 bytes (4600 - 280), then even if we made another
           system call, we couldn't fill the buffer completely, indicating we're at EOF
        */
        self.end_of_stream = no_bytes_left
            || (self.buffer.max_capacity() - size_of::<dirent64>() >= self.remaining_bytes);

        // Reset to start reading from the beginning of the new buffer data
        self.offset = 0;

        // Return true only if we successfully read non-zero bytes
        !no_bytes_left
    }

    /**
    Returns the file descriptor for this directory.

    Useful for operations that need the raw directory FD.
    */
    #[inline]
    pub const fn dirfd(&self) -> &FileDes {
        &self.fd
    }

    #[inline]
    #[allow(clippy::cast_possible_wrap)] // It'll never be high enough (usize->isize)
    /**
      Initiates read-ahead for the directory to improve sequential read performance.

      This system call hints to the kernel that the application intends to read
      the specified range of the directory file soon. The kernel may preload
      this data into the page cache, reducing I/O latency for subsequent reads.

      # Arguments
        `count` - Number of bytes to read ahead from the current offset

      # Returns
      The number of bytes actually read ahead, or -1 on error.

      # Note
      This is an optimisation hint and may be ignored by the kernel.
     Errors are typically silent as read-ahead failures don't affect correctness.
    */
    pub fn readahead(&self, count: usize) -> isize {
        /*  SAFETY:
         - The file descriptor is valid and owned by this struct
         - The offset is within valid bounds for the directory file
         - The count is a valid usize that won't cause arithmetic overflow
         - readahead is a safe syscall that only performs read operationS
        */
        unsafe { libc::readahead(self.fd.0, self.offset as _, count) }
    }

    #[inline]
    pub(crate) fn new(dir: &DirEntry) -> Result<Self> {
        let fd = dir.open_fd()?; //getting the file descriptor
        debug_assert!(fd.is_open(), "We expect it to always be open");

        // SAFETY: The filepath provided is axiomatically less than size `LOCAL_PATH_MAX`
        let (path_buffer, path_len) = unsafe { PathBuffer::init_from_direntry(dir) };
        let buffer = SyscallBuffer::new();
        Ok(Self {
            fd,
            buffer,
            path_buffer,
            file_name_index: path_len,
            parent_depth: dir.depth,
            offset: 0,
            remaining_bytes: 0,
            end_of_stream: false,
        })
    }

    #[inline]
    #[allow(clippy::cast_sign_loss)]
    #[must_use]
    /// Checks if the buffer is empty
    pub const fn is_buffer_not_empty(&self) -> bool {
        self.offset < self.remaining_bytes
    }
}

#[cfg(target_os = "linux")]
impl Iterator for GetDents {
    type Item = DirEntry;
    #[inline]
    /// Returns the next directory entry in the iterator.
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If we have remaining data in buffer, process it
            if self.is_buffer_not_empty() {
                // SAFETY: we've checked it's not null (albeit, implicitly, so deferencing here is fine.)
                let drnt = unsafe { self.get_next_entry() }; //get next entry in the buffer,
                // this is a pointer to the dirent64 structure, which contains the directory entry information
                // SAFETY: we know the pointer is not null therefor the operations in this macro are fine to use.
                skip_dot_or_dot_dot_entries!(drnt.as_ptr(), continue); //provide the continue keyword to skip the current iteration if the entry is invalid or a dot entry
                //extract non . and .. files
                return Some(self.construct_direntry(drnt));
            }

            // Issue a syscall once out of entries
            if self.fill_buffer() {
                continue; // New entries available, restart loop
            }

            return None; //signal end of directory
        }
    }
}
