#![allow(clippy::must_use_candidate)]

use crate::fs::{DirEntry, FileType};
use crate::fs::{FileDes, Result};
use crate::{dirent64, readdir64};
use core::cell::Cell;
use core::ffi::CStr;
use core::ptr::NonNull;
use libc::{AT_SYMLINK_NOFOLLOW, DIR, fstatat};

/**
 POSIX-compliant directory iterator using libc's readdir

 This iterator traverses directory entries using the standard POSIX directory
 reading API. It automatically skips "." and ".." entries and provides
 a safe Rust interface over the underlying C library functions.

*/
#[derive(Debug)]
pub struct ReadDir {
    /// Raw directory pointer from libc's `opendir() wrapped in a nonnull`
    pub(crate) dir: NonNull<DIR>,
    /// Buffer storing the full directory path for constructing entry paths
    pub(crate) path_buffer: Vec<u8>,
    /// Index into `path_buffer` where filenames start (avoids recalculating)
    pub(crate) file_name_index: usize,
    /// Depth of this directory relative to traversal root
    pub(crate) parent_depth: u32,
    /// The file descriptor of this directory, for use in calls like openat/statat etc.
    pub(crate) fd: FileDes,
}

impl ReadDir {
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
    - Unlike the `getdents64`/`getdirentries64` system calls type approach, this implementation uses the
      standard libc directory handling functions
    - The function returns `None` both at end-of-directory and on errors, following
      the traditional `readdir` semantics
    */
    #[inline]
    pub fn get_next_entry(&mut self) -> Option<NonNull<dirent64>> {
        // SAFETY: `self.dir` is a valid directory pointer maintained by the iterator
        let dirent_ptr = unsafe { readdir64(self.dir.as_ptr()) };

        // readdir returns null at end of directory or on error
        NonNull::new(dirent_ptr)
    }

    #[inline]
    pub(crate) fn new(dir_path: &DirEntry) -> Result<Self> {
        let dir = dir_path.opendir()?; //read the directory and get the pointer to the DIR structure.
        let (path_buffer, file_name_index) = Self::init_from_path(dir_path.as_bytes());
        // Mutate the buffer to contain the full path, then add a null terminator and record the new length
        // We use this length to index to get the filename (store full path -> index to get filename)

        // SAFETY: dir is a non null pointer,the pointer is guaranteed to be valid
        let fd = unsafe { FileDes(libc::dirfd(dir.as_ptr())) };
        debug_assert!(fd.is_open(), "We expect it to be open");

        Ok(Self {
            dir,
            path_buffer,
            file_name_index,
            parent_depth: dir_path.depth, //inherit depth
            fd,
        })
    }
}

impl Drop for ReadDir {
    /**
    Closes the directory file descriptor to prevent resource leaks.

    File descriptors are limited system resources, so proper cleanup
    is essential.
    */
    #[inline]
    fn drop(&mut self) {
        debug_assert!(
            self.fd.is_open(),
            "We expect the file descriptor to be open before closing"
        );
        // SAFETY:  not required
        unsafe { libc::closedir(self.dir.as_ptr()) };
        // Basically fdsan shouts about a different object owning the fd, so we close via closedir.
        // unsafe { crate::syscalls::close_asm(self.fd.0) }; // TODO: asm implementation, for when i feel like testing if it does anything useful.
    }
}

/**
Linux/Android-specific directory iterator using the `getdents` system call.

Provides more efficient directory traversal than `readdir` for large directories
by performing batched reads into a kernel buffer. This reduces system call overhead
and improves performance when scanning directories with many entries.

Unlike some directory iteration methods, this does not implicitly call `stat`
on each entry unless required by unusual filesystem behaviour.
*/
#[cfg(any(target_os = "linux", target_os = "android"))]
pub struct GetDents {
    /// File descriptor of the open directory, wrapped for automatic resource management
    pub(crate) fd: FileDes,
    /// Kernel buffer for batch reading directory entries via system call I/O
    /// Approximately 4.1KB in size, optimised for typical directory traversal
    pub(crate) syscall_buffer: crate::fs::types::SyscallBuffer,
    /// Buffer for constructing full entry paths
    /// Reused for each entry to avoid repeated memory allocation (only constructed once per dir)
    pub(crate) path_buffer: Vec<u8>,
    /// Length of the base directory path including the trailing slash
    /// Used for efficient filename extraction and path construction
    pub(crate) file_name_index: usize,
    /// Depth of the parent directory in the directory tree hierarchy
    /// Used to calculate depth for child entries during recursive traversal
    pub(crate) parent_depth: u32,
    /// Current read position within the directory entry buffer
    /// Tracks progress through the currently loaded batch of entries
    pub(crate) offset: usize,
    /// Number of bytes remaining to be processed in the current buffer
    /// Indicates when a new system call is needed to fetch more entries
    pub(crate) remaining_bytes: usize,
    /// A marker for when the `FileDes` can give no more entries
    pub(crate) end_of_stream: bool,
}

#[cfg(any(target_os = "linux", target_os = "android"))]
impl Drop for GetDents {
    /**
      Drops the iterator, closing the file descriptor.
      we need to close the file descriptor when the iterator is dropped to avoid resource leaks.
    */
    #[inline]
    fn drop(&mut self) {
        debug_assert!(
            self.fd.is_open(),
            "We expect the file descriptor to be open before closing"
        );
        // SAFETY: only closing HERE
        unsafe { libc::close(self.fd.0) };
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
impl GetDents {
    /**
    Returns the number of unprocessed bytes remaining in the current kernel buffer.

    This indicates how much data is still available to be processed before needing
    to perform another `getdents64` system call. When this returns 0, the buffer
    has been exhausted.

    # Examples
    ```
    use fdf::fs::DirEntry;
    let start_path=std::env::temp_dir();
    let getdents=DirEntry::new(start_path).unwrap().getdents().unwrap();
    while getdents.remaining_bytes() > 0 {
       // Process entries from current buffer
       }
       // Buffer exhausted, need to read more
       ```

       */
    #[inline]
    pub const fn remaining_bytes(&self) -> usize {
        self.remaining_bytes
    }

    #[inline]
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "hot function, worth some easy optimisation, not caring about 32bit target"
    )]
    pub(crate) fn fill_buffer(&mut self) -> bool {
        // Early return if we've already reached end of stream
        if self.end_of_stream {
            return false;
        }

        // Read directory entries, ignoring negative error codes
        let remaining_bytes = self.syscall_buffer.getdents(&self.fd);

        let has_bytes_remaining = remaining_bytes.is_positive();
        /*
         Use a bit hack to make this statement branchless
         https://graphics.stanford.edu/~seander/bithacks.html#IntegerAbs

        basically equivalent to .max(0) as usize but without branching

        */
        const NUM_OF_BITS_MINUS_1: usize = (usize::BITS - 1) as usize;
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
         - Largest dirent64 size: 280 bytes (Well, see below...)  (it can be up to 4000+ on reiserfs, or 1080~ ish on openzfs( see the filesystem copy paste on this page))
         - If getdents returns ≤ 4320 bytes (4600 - 280), then even if we made another
           system call, it would definitively call 0 bytes on next call, so we skip it!
           Through this optimisation, we can truly 1 shot small directories, as well as remove number of getdents calls down by 50%! (rough tests)
        */

        // Access the last field and then round up to find the minimum struct size
        const MINIMUM_DIRENT_SIZE: usize =
            core::mem::offset_of!(dirent64, d_name).next_multiple_of(8); //==24 on these systems

        // similar to a `static_assert` from c++
        const_assert!(
            MINIMUM_DIRENT_SIZE == 24,
            "minimum dirent size isnt 24 on this system, please report the error"
        );

        // Note, we don't support reiser due to it's massive file name length
        // This should support Openzfs, ZFS is the only FS on linux which has a size greater than 512 bytes
        const MAX_SIZED_DIRENT: usize = 1023 + 1 + MINIMUM_DIRENT_SIZE; // max size of ZFS+NUL + non variable fields
        // Normally the max should be 255 but there's 510 for CIFS or any UTF16 encoded Filesystem
        // Then there's the exception for ZFS with 1023.

        // See proof at bottom of page.
        self.end_of_stream = !has_bytes_remaining
            || self.syscall_buffer.max_capacity() - MAX_SIZED_DIRENT >= self.remaining_bytes; //a boolean

        /*
        you can't have perfection in systems programming, so many variables!
        Ultimately this is a heuristic way, it's not fool proof,
        it won't however miss any entries but it CAN sometimes call `getdents64` to get a 0
        which (officially) indicates EOF

        Actually, it's funny because this optimisation will be even MORE helpful for network file systems!
        */

        // Reset to start reading from the beginning of the new buffer data for the case where it's got
        self.offset = 0;

        // Return true only if we successfully read non-zero bytes
        has_bytes_remaining
    }

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
    #[inline]
    #[expect(clippy::cast_possible_wrap, reason = "not designed for 32bit")]
    #[cfg(target_os = "linux")] // Only available on linux to my knowledge
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
    pub(crate) fn new(dir: &DirEntry) -> Result<Self> {
        let fd = dir.open()?; //getting the file descriptor
        debug_assert!(fd.is_open(), "We expect it to always be open");

        let (path_buffer, file_name_index) = Self::init_from_path(dir);
        let syscall_buffer = crate::fs::types::SyscallBuffer::new();
        Ok(Self {
            fd,
            syscall_buffer,
            path_buffer,
            file_name_index,
            parent_depth: dir.depth,
            offset: 0,
            remaining_bytes: 0,
            end_of_stream: false,
        })
    }

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
    #[inline]
    #[allow(clippy::integer_division_remainder_used)] //debug only
    #[allow(clippy::cast_ptr_alignment)]
    pub fn get_next_entry(&mut self) -> Option<NonNull<dirent64>> {
        loop {
            //we have to use a loop essentially because of the iterative buffer filling semantics, I dislike the complexity!
            // If we have data in buffer, try to get next entry
            if self.offset < self.remaining_bytes {
                // SAFETY: the buffer is not empty and therefore has remaining bytes to be read
                let d: *mut dirent64 =
                    unsafe { self.syscall_buffer.as_ptr().add(self.offset) as _ };

                debug_assert!(
                    d as usize % 8 == 0,
                    "the memory address of the dirent should be SHOULD be  aligned to 8 bytes"
                ); //alignment check
                debug_assert!(!d.is_null(), "dirent is null in get next entry!");
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
    fn parent_depth(&self) -> u32;
    /// Returns the file descriptor for the current directory being read
    fn file_descriptor(&self) -> &FileDes;

    #[inline]
    #[allow(clippy::wildcard_enum_match_arm, reason = "exhaustive")]
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    /// Constructs a `DirEntry` from a raw directory entry pointer
    unsafe fn construct_entry(&mut self, drnt: *const dirent64) -> DirEntry {
        debug_assert!(!drnt.is_null(), "drnt should never be null!");
        // SAFETY: The `drnt` must not be null(by precondition)
        let (f_path, inode, file_type): (&CStr, u64, FileType) =
            unsafe { self.construct_path(drnt) };

        let path: Box<CStr> = f_path.into();
        let file_name_index = self.file_index();

        DirEntry {
            path,
            file_type,
            inode,
            depth: self.parent_depth() + 1,
            file_name_index,
            is_traversible_cache: Cell::new(None), // Lazy cache for traversal checks
        }
    }

    #[inline]
    fn init_from_path(dir_path: &[u8]) -> (Vec<u8>, usize) {
        let mut base_len = dir_path.len(); // get length of directory path

        let is_root = dir_path == b"/";

        let needs_slash = usize::from(!is_root);

        /*
        https://en.wikipedia.org/wiki/Comparison_of_file_systems#Limits

        File System	Maximum Filename Length
        AdvFS	255 characters
        APFS	255 UTF-8 characters
        bcachefs	255 bytes
        BeeGFS	255 bytes
        Btrfs	255 bytes
        EROFS	255 bytes
        ext2	255 bytes
        ext3	255 bytes
        ext4	255 bytes
        F2FS	255 bytes
        FFS	255 bytes
        GFS	255 bytes
        GFS2	255 bytes
        GPFS	255 UTF-8 codepoints
        HFS	31 bytes
        HFS Plus	255 UTF-16 code units /510  bytes
        JFS	255 bytes
        JFS1	255 bytes
        Lustre	255 bytes
        NILFS	255 bytes
        NOVA	255 bytes
        OCFS	255 bytes
        OCFS2	255 bytes
        QFS	255 bytes
        ReiserFS	255 characters // 4032 bytes //not supporting this!
        Reiser4	3976 bytes  //not supporting this!
        UFS1	255 bytes
        UFS2	255 bytes
        VxFS	255 bytes
        XFS	255 bytes
        ZFS	1023 bytes
        NTFS 255 UTF-16 / 510 bytes

        */

        // Max dirent length determined at build time based on supported filesystems
        //Set to to ZFS max (1023) +NUL (this will also support HAMMER/HAMMER2 on dragonflyBSD)
        const MAX_SIZED_DIRENT_LENGTH: usize = 1023 + 1;

        //  Allocate exact size and copy in one operation
        let total_capacity = base_len + needs_slash + MAX_SIZED_DIRENT_LENGTH;
        let mut path_buffer: Vec<u8> = Vec::with_capacity(total_capacity);

        /*  Copy directory path with non-overlapping copy for maximum performance (this is internally a `memcpy`)
         SAFETY:
         - `dir_path.as_ptr()` is valid for reads of `base_len` bytes (source slice length)
         - `path_buffer.as_mut_ptr()` is valid for writes of `base_len` bytes (we allocated `total_capacity >= base_len`)
         - The memory regions are guaranteed non-overlapping: `dir_path` points to existing data
           while `path_buffer` points to freshly allocated memory
         - Both pointers are properly aligned for u8 access
         - `base_len` equals `dir_path.len()`, ensuring we don't read beyond source bounds
        */
        unsafe {
            core::ptr::copy_nonoverlapping(dir_path.as_ptr(), path_buffer.as_mut_ptr(), base_len)
        };
        //https://en.cppreference.com/w/c/string/byte/memcpy (usually I hate cppreference but it's fine for this)
        // from above "memcpy is the fastest library routine for memory-to-memory copy"

        // SAFETY: We've allocated enough capacity and only need to set the length
        // The filename portion will be overwritten during iteration
        unsafe { path_buffer.set_len(total_capacity) };

        /*
        Essentially  what we're doing here is creating 1 vector per  directory, with enough space allocated to hold any filename
        This allows no dynamic resizing during iteration, which is costly!
         */

        #[allow(clippy::multiple_unsafe_ops_per_block)] //dumb
        // SAFETY: write is within buffer bounds
        unsafe {
            *path_buffer.as_mut_ptr().add(base_len) = b'/' //this doesnt matter for non directories, since we're overwriting it anyway
        };

        base_len += needs_slash; // update length if slash added (we're tracking the baselen, we dont care about the slash on the end because we're truncating it anyway)

        (path_buffer, base_len)
    }

    /**
    Constructs a full path by appending the directory entry name to the base path

    returns the full path, inode,`FileType` (not abstracted into types bc of  internal use only)
    */
    #[inline]
    unsafe fn construct_path(&mut self, drnt: *const dirent64) -> (&CStr, u64, FileType) {
        // Note to adrian, I refactored this to avoid cache misses.
        debug_assert!(!drnt.is_null(), "drnt is null in construct path!");
        // SAFETY: The `drnt` must not be null (checked before using)
        let d_name: *const u8 = unsafe { access_dirent!(drnt, d_name) };
        // SAFETY: same as above
        let d_ino: u64 = unsafe { access_dirent!(drnt, d_ino) };
        // SAFETY: as above.
        let dtype: u8 = unsafe { access_dirent!(drnt, d_type) }; //need to optimise this for illumos/solaris TODO! (small nit)
        // SAFETY: Same as above^
        // Add 1 to include the null terminator
        let name_len = unsafe { crate::util::dirent_name_length(drnt) + 1 }; //technically should be a u16 but we need it for indexing :(
        let base_len: usize = self.file_index();

        // if d_type==`DT_UNKNOWN`  then make an fstat at call to determine
        #[allow(clippy::wildcard_enum_match_arm)] // ANYTHING but unknown is fine.
        let file_type: FileType = match FileType::from_dtype(dtype) {
            FileType::Unknown => stat_syscall!(
                fstatat,
                self.file_descriptor().0, //borrow before mutably borrowing the path buffer
                d_name.cast(), //cast into i8 (depending on architecture, pointers are either i8/u8)
                AT_SYMLINK_NOFOLLOW, // dont follow, to keep same semantics as readdir/getdents
                DTYPE
            ),
            not_unknown => not_unknown, //if not unknown, skip the syscall (THIS IS A MASSIVE PERF WIN)
        };
        let path_buffer: &mut Vec<u8> = self.path_buffer();
        // SAFETY: The `base_len` is guaranteed to be a valid index into `path_buffer`
        let buffer: &mut [u8] = unsafe { path_buffer.get_unchecked_mut(base_len..) };

        // SAFETY: `d_name` and `buffer` don't overlap (different memory regions)
        // - Both pointers are properly aligned for byte copying
        // - `name_len` is within `buffer` bounds
        unsafe { core::ptr::copy_nonoverlapping(d_name, buffer.as_mut_ptr(), name_len) };
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        // SAFETY: the buffer is guaranteed null terminated and we're accessing in bounds
        unsafe {
            (
                CStr::from_bytes_with_nul_unchecked(
                    path_buffer.get_unchecked(..base_len + name_len),
                ),
                d_ino,
                file_type,
            )
        }
    }
}

// Cheap macro to avoid duplicate code maintenance.
macro_rules! impl_iter {
    ($struct:ty) => {
        impl $struct {
            /**
            Returns the file descriptor for this directory.

            Useful for operations that need the raw directory FD.

            ISSUE: this file descriptor is only closed by the iterator due to current limitations
            */
            #[inline]
            pub const fn dirfd(&self) -> &$crate::fs::FileDes {
                &self.fd
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
            pub fn construct_direntry(
                &mut self,
                drnt: core::ptr::NonNull<$crate::dirent64>,
            ) -> $crate::fs::DirEntry {
                // SAFETY:  Because the pointer is already checked to not be null before it can be used here safely
                unsafe { self.construct_entry(drnt.as_ptr()) }
            }
        }
    };
}

// Simple repetition avoider
macro_rules! impl_dirent_constructor {
    ($type:ty) => {
        impl DirentConstructor for $type {
            #[inline]
            fn path_buffer(&mut self) -> &mut Vec<u8> {
                &mut self.path_buffer
            }

            #[inline]
            fn file_index(&self) -> usize {
                self.file_name_index
            }

            #[inline]
            fn parent_depth(&self) -> u32 {
                self.parent_depth
            }

            #[inline]
            fn file_descriptor(&self) -> &$crate::fs::FileDes {
                &self.fd
            }
        }
    };
}

macro_rules! impl_iterator_for_dirent {
    ($type:ty) => {
        impl Iterator for $type {
            type Item = $crate::fs::DirEntry;

            #[inline]
            fn next(&mut self) -> Option<Self::Item> {
                while let Some(drnt) = self.get_next_entry() {
                    skip_dot_or_dot_dot_entries!(drnt.as_ptr(), continue); // this just skips dot entries in a really efficient manner(avoids strlen)
                    return Some(self.construct_direntry(drnt));
                }
                None // signal end of directory
            }
        }
    };
}

// Common to all platforms
impl_iter!(ReadDir);
impl_iterator_for_dirent!(ReadDir);
impl_dirent_constructor!(ReadDir);

// Linux/Android specific
#[cfg(any(target_os = "linux", target_os = "android"))]
impl_iter!(GetDents);
#[cfg(any(target_os = "linux", target_os = "android"))]
impl_iterator_for_dirent!(GetDents);
#[cfg(any(target_os = "linux", target_os = "android"))]
impl_dirent_constructor!(GetDents);
