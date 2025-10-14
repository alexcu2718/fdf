#![allow(clippy::must_use_candidate)]
use crate::FileType;
#[cfg(target_os = "linux")]
use crate::SyscallBuffer;
use crate::{DirEntry, FileDes, Result};
use core::cell::Cell;
use core::ffi::CStr;
use core::ptr::NonNull;
use libc::DIR;
#[cfg(not(target_os = "linux"))]
use libc::{dirent as dirent64, readdir};
#[cfg(target_os = "linux")]
use libc::{dirent64, readdir64 as readdir};

const_from_env!(FDF_MAX_FILENAME_LEN:usize="FDF_MAX_FILENAME_LEN",512); //setting the minimum extra memory it'll need
// this should be ideally 256 but operating systemns can be funky, so i'm being a bit cautious
// as this is written for unix, most except HURD i think allow the filename to be 255 max, I'm not writing this for hurd ffs.
const _: () = assert!(
    //0 cost compile time check that doesnt rely on debug
    FDF_MAX_FILENAME_LEN >= 255,
    "Expect it to always be above this value"
);
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
    pub(crate) path_buffer: Vec<u8>,
    /// Index into `path_buffer` where filenames start (avoids recalculating)
    pub(crate) file_name_index: usize,
    /// Depth of this directory relative to traversal root
    pub(crate) parent_depth: u16,
    /// The file descriptor of this directory, for use in calls like openat/statat etc.
    pub(crate) dirfd: FileDes,
}

impl ReadDir {
    #[inline]
    /**
    Reads the next directory entry using the libc `readdir` function.

    This function provides a safe wrapper around the libc `readdir` call, advancing
    the directory stream and returning a pointer to the next directory entry.

    The function handles the underlying directory stream management automatically,
    including positioning and error conditions.

    IMPORTANT: This function returns ALL directory entries, including "." and ".." entries.
    Filtering of these entries should be handled by the caller if desired.

    # Returns
    - `Some(NonNull<dirent64>)` when a directory entry is successfully read
    - `None` when the end of directory is reached or if an error occurs

    # Notes
    - Unlike the `getdents64` system call approach, this implementation uses the
      standard libc directory handling functions
    - The function returns `None` both at end-of-directory and on errors, following
      the traditional `readdir` semantics
    */
    pub fn get_next_entry(&mut self) -> Option<NonNull<dirent64>> {
        // SAFETY: `self.dir` is a valid directory pointer maintained by the iterator
        let dirent_ptr = unsafe { readdir(self.dir.as_ptr()) };

        // readdir returns null at end of directory or on error
        NonNull::new(dirent_ptr)
    }

    #[inline]
    /// Provides read only access to the internal buffer that holds the path used to iterate with
    pub const fn borrow_path_buffer(&self) -> &Vec<u8> {
        &self.path_buffer
    }

    /**
     Determines the file type of a directory entry with fallback resolution.

     This method attempts to determine the file type using the directory entry's
     `d_type` field when available, with a fallback to fstat-based resolution
     when the type is unknown or unsupported by the filesystem.

     # Arguments
     * `d_type` - The file type byte from the directory entry's `d_type` field;This corresponds to DT_* constants in libc (e.g., `DT_REG`, `DT_DIR`).
     * `filename` - The filename as a C string, used for fallback stat resolution;when `d_type` is `DT_UNKNOWN`

     # Returns
     A `FileType` enum variant representing the determined file type.

     # Behavior
     - **Fast Path**: When `d_type` contains a known file type (not `DT_UNKNOWN`),
       returns the corresponding `FileType` without additional system calls.
     - **Fallback Path**: When `d_type` is `DT_UNKNOWN`, performs a `fstat` call
       on the file to determine its actual type.
     - **Symlink Handling**: For `DT_LNK`, returns `FileType::Symlink` directly
       without following the link.

     # Examples
     ```ignore
     // These tests are disabled due to macos pecularities with tmp dir's
     use std::ffi::CStr;
     use fdf::{DirEntry, FileType};
     use std::env::temp_dir;
     let tmpdir=temp_dir();
     let tmp=tmpdir.as_os_str();
     let direntry=DirEntry::new(tmp).unwrap();
     let dir = direntry.readdir().unwrap();
     let filename = c"filedoesnot exist";

     // With known file type (regular file)
     let file_type = dir.get_filetype(libc::DT_REG, filename);
     assert!(file_type.is_regular_file());

     // With unknown type requiring stat fallback
     let file_type = dir.get_filetype(libc::DT_UNKNOWN, filename);
     // Internally calls stat() to determine actual file type
     ```

     # Performance Notes
     - Prefer using directory entries with supported `d_type` to avoid stat calls
     - The fallback stat call adds filesystem overhead but ensures correctness
     - Some filesystems (e.g., older XFS, NTFS) may (I haven't checked) return `DT_UNKNOWN`
    */
    #[inline]
    pub fn get_filetype(&self, d_type: u8, filename: &CStr) -> FileType {
        self.get_filetype_private(d_type, filename) //use an internal trait because i want to avoid code deduplication.
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
        let dir_stream = dir_path.opendir()?; //read the directory and get the pointer to the DIR structure.
        let (path_buffer, path_len) = Self::init_from_direntry(dir_path);
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
    /**
     Closes the directory file descriptor to prevent resource leaks.

     File descriptors are limited system resources, so proper cleanup
     is essential.
    */
    fn drop(&mut self) {
        debug_assert!(
            self.dirfd.is_open(),
            "We expect the file descriptor to be open before closing"
        );
        // SAFETY:  not required
        unsafe { libc::closedir(self.dir.as_ptr()) };
        // Basically fdsan shouts about a different object owning the fd, so we close via closedir.
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
    pub(crate) syscall_buffer: SyscallBuffer,
    /// buffer for constructing full entry paths
    /// Reused for each entry to avoid repeated memory allocation (only constructed once per dir)
    pub(crate) path_buffer: Vec<u8>,
    /// Length of the base directory path including the trailing slash
    /// Used for efficient filename extraction and path construction
    pub(crate) file_name_index: usize,
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
        // SAFETY:  not required
        unsafe { libc::close(self.fd.0) };
        //unsafe { crate::syscalls::close_asm(self.fd.0) }; //asm implementation, for when i feel like testing if it does anything useful.
    }
}

#[cfg(target_os = "linux")]
impl GetDents {
    #[inline]
    #[must_use]
    /**
     Returns whether the directory stream has reached its end.

     This method indicates that no more directory entries can be read from this stream.
     Once `true`, subsequent calls to [`fill_buffer()`](Self::fill_buffer) will return `false`
     and the iterator will return `None`.

     # Returns
     - `true` if the directory stream has ended (EOF reached or buffer capacity insufficient)
     - `false` if more directory entries may be available to read

     # Examples
     ```
     use fdf::DirEntry;
     let dir = DirEntry::new("/tmp").unwrap();
     let mut getdents = dir.getdents().unwrap();

     // Process entries until end of stream
     while !getdents.is_end_of_stream() {
         if getdents.fill_buffer() {
             // Process entries in buffer...
         }
     }
     println!("Reached end of directory");
     ```
    */
    pub const fn is_end_of_stream(&self) -> bool {
        self.end_of_stream
    }

    /**
     Determines the file type of a directory entry with fallback resolution.

     This method attempts to determine the file type using the directory entry's
     `d_type` field when available, with a fallback to fstat-based resolution
     when the type is unknown or unsupported by the filesystem.

     # Arguments
     * `d_type` - The file type byte from the directory entry's `d_type` field;This corresponds to DT_* constants in libc (e.g., `DT_REG`, `DT_DIR`).
     * `filename` - The filename as a C string, used for fallback stat resolution;when `d_type` is `DT_UNKNOWN`

     # Returns
     A `FileType` enum variant representing the determined file type.

     # Behavior
     - **Fast Path**: When `d_type` contains a known file type (not `DT_UNKNOWN`),
       returns the corresponding `FileType` without additional system calls.
     - **Fallback Path**: When `d_type` is `DT_UNKNOWN`, performs a `fstat` call
       on the file to determine its actual type.
     - **Symlink Handling**: For `DT_LNK`, returns `FileType::Symlink` directly
       without following the link.

     # Examples
     ```no_run
     use std::ffi::CStr;
     use fdf::{DirEntry, FileType};
     use std::env::temp_dir;
     let tmpdir=temp_dir();
     let tmp=tmpdir.as_os_str();
     let direntry=DirEntry::new(tmp).unwrap();
     let dir = direntry.getdents().unwrap();
     let filename = c"filedoesnot exist";

     // With known file type (regular file)
     let file_type = dir.get_filetype(libc::DT_REG, filename);
     assert!(file_type.is_regular_file());

     // With unknown type requiring stat fallback
     let file_type = dir.get_filetype(libc::DT_UNKNOWN, filename);
     // Internally calls stat() to determine actual file type
     ```

     # Performance Notes
     - Prefer using directory entries with supported `d_type` to avoid stat calls
     - The fallback stat call adds filesystem overhead but ensures correctness
     - Some filesystems (e.g., older XFS, NTFS) may (I haven't checked) return `DT_UNKNOWN`
    */
    #[inline]
    pub fn get_filetype(&self, d_type: u8, filename: &CStr) -> FileType {
        self.get_filetype_private(d_type, filename) //use an internal trait because i want to avoid code deduplication.
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
     has been exhausted.

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
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "hot function, worth some easy optimisation, not caring about 32bit target"
    )]
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
    pub(crate) fn fill_buffer(&mut self) -> bool {
        // Early return if we've already reached end of stream
        if self.is_end_of_stream() {
            return false;
        }

        // Read directory entries, ignoring negative error codes
        let remaining_bytes = self.syscall_buffer.getdents(&self.fd);

        let has_bytes_remaining = remaining_bytes.is_positive();
        /*
         Use a bit hack to make this statement branchless
         https://graphics.stanford.edu/~seander/bithacks.html#IntegerAbs


        */
        const NUM_OF_BITS_MINUS_1: usize = 8 * core::mem::size_of::<usize>() - 1;
        self.remaining_bytes =
            (remaining_bytes & !(remaining_bytes >> NUM_OF_BITS_MINUS_1)) as usize;

        /*

         Smart end-of-stream detection: Avoid unnecessary system calls by detecting when
         we've likely exhausted the directory based on the returned byte count.

         Why this works:
         - A full directory read returns exactly buffer.max_capacity() bytes
         - A partial read (end approaching) returns less than maximum
         - If returned bytes ≤ (max_capacity - largest_dirent_size), the file descriptor is exhausted
         - Meaning that the next system call will return 0 anyway.

         Example:
         - Buffer capacity: 4600 bytes (It is arbitrary)
         - Largest dirent64 size: 280 bytes
         - If getdents returns ≤ 4320 bytes (4600 - 280), then even if we made another
           system call, it would definitively call 0 bytes on next call, so we skip it!
           Through this optimisation, we can truly 1 shot small directories, as well as remove number of getdents calls down by 50%! (rough tests)
        */
        self.end_of_stream = !has_bytes_remaining
            || self.syscall_buffer.max_capacity() - size_of::<dirent64>() >= self.remaining_bytes; //a boolean

        // Reset to start reading from the beginning of the new buffer data for the case where it's got
        self.offset = 0;

        // Return true only if we successfully read non-zero bytes
        has_bytes_remaining
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
    #[expect(clippy::cast_possible_wrap, reason = "not designed for 32bit")]
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
        // Note, not used yet but will be.
    }

    #[inline]
    /// Provides read only access to the internal buffer that holds the path used to iterate with
    pub const fn borrow_path_buffer(&self) -> &Vec<u8> {
        &self.path_buffer
    }
    #[inline]
    /// Provides read only access to the internal buffer that holds the bytes read from the syscall
    pub const fn borrow_syscall_buffer(&self) -> &SyscallBuffer {
        &self.syscall_buffer
    }

    #[inline]
    pub(crate) fn new(dir: &DirEntry) -> Result<Self> {
        let fd = dir.open()?; //getting the file descriptor
        debug_assert!(fd.is_open(), "We expect it to always be open");

        let (path_buffer, path_len) = Self::init_from_direntry(dir);
        let buffer = SyscallBuffer::new();
        Ok(Self {
            fd,
            syscall_buffer: buffer,
            path_buffer,
            file_name_index: path_len,
            parent_depth: dir.depth,
            offset: 0,
            remaining_bytes: 0,
            end_of_stream: false,
        })
    }
    #[inline]
    #[allow(clippy::cast_ptr_alignment)]
    /**
        Advances the iterator to the next directory entry in the buffer and returns a pointer to it.

        This function processes the internal buffer filled by `getdents64` system calls, interpreting
        the data at the current offset as a `dirent64` structure. After reading an entry, the internal
        offset is advanced by the entry's record length (`d_reclen`), positioning the iterator for
        the next subsequent call.

        IMPORTANT: This function returns ALL directory entries, including "." and ".." entries.
        Filtering of these entries should be handled by the caller if desired.

        # Returns
        - `Some(NonNull<dirent64>)` when a valid directory entry is available
        - `None` when the buffer is exhausted and no more entries can be read

        # Behavior
        The function performs the following steps:
        1. Checks if unread data remains in the internal buffer
        2. Casts the current buffer position to a `dirent64` pointer
        3. Extracts the entry's record length to advance the internal offset
        4. Returns a non-null pointer wrapped in `Some`, or `None` at buffer end
    */
    pub fn get_next_entry(&mut self) -> Option<NonNull<dirent64>> {
        loop {
            //we have to use a loop essentially because of the iterative buffer filling semantics, I dislike the complexity!
            // If we have data in buffer, try to get next entry
            if self.offset < self.remaining_bytes {
                // SAFETY: the buffer is not empty and therefore has remaining bytes to be read
                let d: *mut dirent64 =
                    unsafe { self.syscall_buffer.as_ptr().add(self.offset) as _ };
                // SAFETY: dirent is not null so field access is safe
                let reclen = unsafe { access_dirent!(d, d_reclen) };

                self.offset += reclen; // increment the offset by the size of the dirent structure

                // SAFETY: dirent is not null
                return unsafe { Some(NonNull::new_unchecked(d)) };
            }

            // Buffer is empty, try to fill it
            if !self.fill_buffer() {
                return None; // No more data to read
            }
            // Buffer filled successfully, loop to try reading again
        }
    }
}

#[cfg(target_os = "linux")]
impl Iterator for GetDents {
    type Item = DirEntry;
    #[inline]
    /// Returns the next directory entry in the iterator.
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.get_next_entry() {
                Some(drnt) => {
                    // this just skips dot entries in a really efficient manner(avoids strlen)
                    skip_dot_or_dot_dot_entries!(drnt.as_ptr(), continue);
                    return Some(self.construct_direntry(drnt));
                }
                None => return None, // signal end of directory
            }
        }
    }
}

/**
  Internal trait for constructing directory entries during iteration

 This trait provides the necessary components to construct `DirEntry` objects
 from raw `dirent64` structures while maintaining path buffer state, tracking
 file name positions, and managing directory traversal depth.

*/
pub trait DirentConstructor {
    /// Returns a mutable reference to the path buffer used for constructing full paths
    fn path_buffer(&mut self) -> &mut Vec<u8>;
    /// Returns the current index in the path buffer where the filename should be appended
    ///
    /// This represents the length of the base directory path before adding the current filename.
    fn file_index(&self) -> usize; //modify name a bit so we dont get collisions.
    /// Returns the depth of the parent directory in the traversal hierarchy
    ///
    /// Depth starts at 0 for the root directory being scanned and increments for each subdirectory.
    fn parent_depth(&self) -> u16;
    /// Returns the file descriptor for the current directory being read
    fn file_descriptor(&self) -> &FileDes;

    #[inline]
    /// Constructs a `DirEntry` from a raw directory entry pointer
    #[allow(unused_unsafe)] //lazy fix for illumos/solaris (where we dont actually dereference the pointer, just return unknown TODO-MAKE MORE ELEGANT)
    unsafe fn construct_entry(&mut self, drnt: *const dirent64) -> DirEntry {
        let base_len = self.file_index();
        // SAFETY: The `drnt` must not be null (checked before using)
        let dtype = unsafe { access_dirent!(drnt, d_type) }; //need to optimise this for illumos/solaris TODO!
        // SAFETY: Same as above^
        let inode = unsafe { access_dirent!(drnt, d_ino) };

        // SAFETY: The `drnt` must not be null(by precondition)
        let full_path = unsafe { self.construct_path(drnt) };
        let path: Box<CStr> = full_path.into();
        let file_type = self.get_filetype_private(dtype, &path);

        DirEntry {
            path,
            file_type,
            inode,
            depth: self.parent_depth() + 1,
            file_name_index: base_len as _,
            is_traversible_cache: Cell::new(None), //// Lazy cache for traversal checks
        }
    }

    #[inline]
    fn init_from_direntry(dir_path: &DirEntry) -> (Vec<u8>, usize) {
        let dir_path_in_bytes = dir_path.as_bytes();
        let mut base_len = dir_path_in_bytes.len(); // get length of directory path

        let is_root = dir_path_in_bytes == b"/";

        let needs_slash_u8 = u8::from(!is_root); // check if we need to append a slash
        let needs_slash: usize = usize::from(needs_slash_u8);
        //set a conservative estimate incase it returns something useless
        // Initialise buffer with zeros to avoid uninitialised memory then add the max length of a filename on
        let mut path_buffer = vec![0u8; base_len + needs_slash + FDF_MAX_FILENAME_LEN + 10]; //add 10 for good measure negligible performance cost,
        // Please note future readers, `PATH_MAX` is not the max length of a path, it's simply the maximum length of a path that POSIX functions will take
        // I made this mistake then suffered a segfault to the knee. BEWARB
        let buffer_ptr = path_buffer.as_mut_ptr(); // get the mutable pointer to the buffer
        // SAFETY: the memory regions do not overlap , src and dst are both valid and alignment is trivial (u8)
        unsafe { core::ptr::copy_nonoverlapping(dir_path_in_bytes.as_ptr(), buffer_ptr, base_len) }; // copy path

        /*
        Essentially  what we're doing here is creating 1 vector per  directory, with enough space allocated to hold any filename
        This allows no dynamic resizing during iteration, which could be costly!
         */

        #[allow(clippy::multiple_unsafe_ops_per_block)] //dumb
        // SAFETY: write is within buffer bounds
        unsafe {
            *buffer_ptr.add(base_len) = b'/' * needs_slash_u8 // add slash if needed  (this avoids a branch ), either add 0 or  add a slash (multiplication)
        };

        base_len += needs_slash; // update length if slash added

        (path_buffer, base_len)
    }

    #[inline]
    /**
      Constructs a full path by appending the directory entry name to the base path


    */
    unsafe fn construct_path(&mut self, drnt: *const dirent64) -> &CStr {
        let base_len = self.file_index();
        // SAFETY: The `drnt` must not be null (checked before using)
        let d_name = unsafe { access_dirent!(drnt, d_name) };
        // SAFETY: as above
        // Add 1 to include the null terminator
        let name_len = unsafe { crate::utils::dirent_name_length(drnt) + 1 };

        let path_buffer = self.path_buffer();
        // SAFETY: The `base_len` is guaranteed to be a valid index into `path_buffer`
        let buffer = unsafe { &mut path_buffer.get_unchecked_mut(base_len..) };
        // SAFETY:
        // - `d_name` and `buffer` don't overlap (different memory regions)
        // - Both pointers are properly aligned for byte copying
        // - `name_len` is within `buffer` bounds (checked by debug assertion)
        unsafe { core::ptr::copy_nonoverlapping(d_name, buffer.as_mut_ptr(), name_len) };

        /*
         SAFETY: the buffer is guaranteed null terminated and we're accessing in bounds
        */
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            CStr::from_bytes_with_nul_unchecked(path_buffer.get_unchecked(..base_len + name_len))
        }
    }

    #[inline]
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    #[allow(clippy::wildcard_enum_match_arm)]
    fn get_filetype_private(&self, d_type: u8, path: &CStr) -> FileType {
        match FileType::from_dtype(d_type) {
            FileType::Unknown => {
                // Fall back to fstatat for filesystems that don't provide d_type (DT_UNKNOWN)
                /* SAFETY:
                - `file_index()` points to the start of the file name within `bytes`
                - The slice from this index to the end includes the null terminator
                - The slice is guaranteed to represent a valid C string (thus null terminated) */
                let cstr_name: &CStr = unsafe {
                    CStr::from_bytes_with_nul_unchecked(
                        path.to_bytes_with_nul().get_unchecked(self.file_index()..),
                    )
                };
                FileType::from_fd_no_follow(self.file_descriptor(), cstr_name)
            }
            known_type => known_type,
        }
    }
}

impl DirentConstructor for ReadDir {
    #[inline]
    fn path_buffer(&mut self) -> &mut Vec<u8> {
        &mut self.path_buffer
    }

    #[inline]
    fn file_index(&self) -> usize {
        self.file_name_index
    }

    #[inline]
    fn parent_depth(&self) -> u16 {
        self.parent_depth
    }

    #[inline]
    fn file_descriptor(&self) -> &FileDes {
        self.dirfd()
    }
}

#[cfg(target_os = "linux")]
impl DirentConstructor for GetDents {
    #[inline]
    fn path_buffer(&mut self) -> &mut Vec<u8> {
        &mut self.path_buffer
    }

    #[inline]
    fn file_index(&self) -> usize {
        self.file_name_index
    }

    #[inline]
    fn parent_depth(&self) -> u16 {
        self.parent_depth
    }
    #[inline]
    fn file_descriptor(&self) -> &FileDes {
        &self.fd
    }
}
