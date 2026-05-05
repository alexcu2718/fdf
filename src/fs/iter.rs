#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris"
))]
use crate::fs::types::SyscallBuffer;
use crate::fs::{DirEntry, FileDes, FileType, Result};
use crate::{Unique, dirent64, readdir64};
use core::cell::Cell;
use core::ffi::CStr;
use core::ptr::NonNull;
use libc::closedir;
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

    This was *MAINLY* implemented to give a lower level interface so that one can use `std::iter::from_fn`
    It's not meant to be used without explicit reason.

    IMPORTANT: This function returns ALL directory entries, including "." and ".." entries.
    Filtering of these entries should be handled by the caller if desired.

    # Returns
    - `Some(Unique<dirent64>)` when a directory entry is successfully read
    - `None` when the end of directory is reached or if an error occurs



    # Notes
    - Unlike the `getdents64`/`getdirentries64` system calls type approach, this implementation uses the
      standard libc directory handling functions
    - The function returns `None` both at end-of-directory and on errors, following
      the traditional `readdir` semantics
    */
    #[inline]
    pub fn get_next_entry(&mut self) -> Option<Unique<dirent64>> {
        // SAFETY: `self.dir` is a valid directory pointer maintained by the iterator
        let dirent_ptr = unsafe { readdir64(self.dir.as_ptr()) };

        // readdir returns null at end of directory or on error
        Unique::new(dirent_ptr)
    }

    #[inline]
    pub(crate) fn new(dir_path: &DirEntry) -> Result<Self> {
        let fd = dir_path.open()?; //read file descriptor
        let (path_buffer, file_name_index) = Self::init_from_path(dir_path);
        // Mutate the buffer to contain the full path, then add a null terminator and record the new length
        // We use this length to index to get the filename (store full path -> index to get filename)

        // SAFETY: the fd was opened  with `O_DIRECTORY`, this  is guaranteed to be valid.
        let dir = unsafe { NonNull::new_unchecked(libc::fdopendir(fd.0)) };
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
        // SAFETY: only closing HERE
        #[cfg(not(debug_assertions))]
        unsafe {
            closedir(self.dir.as_ptr())
        };
        #[cfg(debug_assertions)]
        assert!(
            // SAFETY: as above
            unsafe { closedir(self.dir.as_ptr()) } == 0,
            "Fd was not closed in readdir!"
        );
    }
    // Basically fdsan shouts about a different object owning the fd, so we close via closedir.
    // This is because it's UB to close via file descriptor according to GNU docs, if that file descriptor
    // was obtained from the `dirfd()`
    //( i want to pass ownership to the FileDes BUT due to above limitations, I need a different approach
    // TODO!
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
    /// Constructs a `DirEntry` from a raw directory entry pointer
    fn construct_entry(&mut self, drnt: Unique<dirent64>) -> DirEntry {
        let (cstrpath, inode, file_type): (&CStr, u64, FileType) = self.construct_path(drnt);

        DirEntry {
            path: cstrpath.into(),
            file_type,
            inode,
            depth: self.parent_depth() + 1,
            file_name_index: self.file_index(),
            is_traversible_cache: Cell::new(None), // Lazy cache for traversal checks
        }
    }

    #[inline]
    fn init_from_path(path: &[u8]) -> (Vec<u8>, usize) {
        let mut base_len = path.len(); // get length of directory path

        // Quicker shortcircuit
        let is_root = base_len == 1 && path == b"/";

        let needs_slash = usize::from(!is_root);

        // Fast-path filename capacity (+NUL is included in `name_len` during append).
        // Longer names take the cold slow-path reserve in `construct_path`.
        // Most filepaths will never be longer than this. In the odd-case they are, it's really rare
        // with no negligible affect otherwise
        const FAST_PATH_DIRENT_LENGTH: usize = 256;

        //  Allocate exact size and copy in one operation
        let total_capacity = base_len + needs_slash + FAST_PATH_DIRENT_LENGTH;
        let mut path_buffer: Vec<u8> = Vec::with_capacity(total_capacity);

        /*  Copy directory path with non-overlapping copy for maximum performance (this is internally a `memcpy`)
         SAFETY:
         - `path.as_ptr()` is valid for reads of `base_len` bytes (source slice length)
         - `path_buffer.as_mut_ptr()` is valid for writes of `base_len` bytes (we allocated `total_capacity >= base_len`)
         - The memory regions are guaranteed non-overlapping: `path` points to existing data
           while `path_buffer` points to freshly allocated memory
         - Both pointers are properly aligned for u8 access
         - `base_len` equals `path.len()`, ensuring we don't read beyond source bounds
        */
        unsafe {
            path.as_ptr()
                .copy_to_nonoverlapping(path_buffer.as_mut_ptr(), base_len)
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

        // SAFETY: write is within buffer bounds
        unsafe {
            path_buffer.as_mut_ptr().add(base_len).write(b'/') //this doesnt matter for non directories, since we're overwriting it anyway
        };

        base_len += needs_slash;
        // update length if slash added(we're tracking the baselen, we dont care about the slash on the end because we're truncating it anyway)

        (path_buffer, base_len)
    }

    /**
    Constructs a full path by appending the directory entry name to the base path

    returns the full path, inode,`FileType` (not abstracted into types bc of  internal use only)
    */
    #[inline]
    fn construct_path(&mut self, drnt: Unique<dirent64>) -> (&CStr, u64, FileType) {
        let d_name = drnt.d_name();
        let d_ino = drnt.d_ino(); // Returns 0 if d_ino isn't defined on your system

        // Add 1 to include the null terminator
        let name_len = drnt.name_length() + 1; //technically should be a u16 but we need it for indexing :(

        // if d_type==`DT_UNKNOWN`  then make an fstat at call to determine
        #[cfg(has_d_type)]
        let file_type: FileType = match FileType::from_dtype(drnt.d_type()) {
            FileType::Unknown => stat_syscall!(
                fstatat,
                self.file_descriptor().0, //borrow before mutably borrowing the path buffer
                d_name.cast(), //cast into i8 (depending on architecture, pointers are either i8/u8)
                AT_SYMLINK_NOFOLLOW, // dont follow, to keep same semantics as readdir/getdents
                DTYPE
            ),
            not_unknown => not_unknown, //if not unknown, skip the syscall (THIS IS A MASSIVE PERF WIN)
        };

        #[cfg(not(has_d_type))] // Have to make a syscall on these systems alas
        let file_type = stat_syscall!(
            fstatat,
            self.file_descriptor().0, //borrow before mutably borrowing the path buffer
            d_name.cast(), //cast into i8 (depending on architecture, pointers are either i8/u8)
            AT_SYMLINK_NOFOLLOW, // dont follow, to keep same semantics as readdir/getdents
            DTYPE
        );

        #[cold]
        #[inline(never)]
        fn reserve_for_long_name(path_buffer: &mut Vec<u8>, required_len: usize) {
            let current_len = path_buffer.len();
            path_buffer.reserve_exact(required_len - current_len);
            // SAFETY: we reserved enough capacity and bytes in the extended range
            // are immediately written by `copy_to_nonoverlapping`.
            unsafe { path_buffer.set_len(required_len) };
        }

        let base_len = self.file_index();
        let required_len = base_len + name_len;

        let path_buffer = self.path_buffer();
        if required_len > path_buffer.len() {
            reserve_for_long_name(path_buffer, required_len);
        }

        // Get the portion of the buffer that goes past the last slash
        // SAFETY: The `base_len` is guaranteed to be a valid index into `path_buffer`
        let buffer: &mut [u8] = unsafe { path_buffer.get_unchecked_mut(base_len..) };

        // SAFETY: `d_name` and `buffer` don't overlap (different memory regions)
        // - Both pointers are properly aligned for byte copying
        // - `name_len` is within `buffer` bounds
        // Copy the name into the final portion
        unsafe {
            d_name
                .cast::<u8>()
                .copy_to_nonoverlapping(buffer.as_mut_ptr(), name_len)
        };
        // SAFETY: the buffer is guaranteed null terminated and we're accessing in bounds
        let full_path = unsafe {
            CStr::from_bytes_with_nul_unchecked(path_buffer.get_unchecked(..required_len))
        }; //truncate the buffer to the first null terminator of the full path

        (full_path, d_ino, file_type)
    }
}

/**
High-throughput directory iterator backed by `getdents` or `getdirentries`,
 depending on the target platform.

 This implementation reads directory entries in batches directly from the kernel,
 which reduces libc overhead and is typically faster than `readdir` when walking
 large directories.

 It also avoids implicit `stat` calls for each entry and only falls back to
 metadata lookups when the filesystem does not provide a usable entry type.
*/
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "freebsd",
    target_os = "macos"
))]
pub struct GetDents {
    /// File descriptor of the open directory, wrapped in a `New Type`, does not implement Drop(maydo at later point),
    /// The iterator closes the file descriptor upon this struct beying dropped.
    pub(crate) fd: FileDes,
    /// Kernel buffer for batch reading directory entries via system call I/O
    /// typically using the best calculated  buffer sizes, optimised for typical directory traversal (derived from syscall tracing)
    pub(crate) syscall_buffer: SyscallBuffer,
    /// buffer for constructing full entry paths
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
    #[cfg(any(target_os = "freebsd", target_os = "macos"))] // TODO add dragonflyBSD here eventually
    /// The base pointer for the getdirentries call
    pub(crate) base_pointer: i64,
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "freebsd",
    target_os = "macos"
))]
impl GetDents {
    #[inline]
    /// Convenience function for pointer arithmetic on the buffer
    pub(crate) const unsafe fn buffer_add(&self, amt: usize) -> *const u8 {
        // SAFETY:  internal use only, the `amt` parameter is always within bounds of the buffer.
        unsafe { self.syscall_buffer.as_ptr().byte_add(amt) }
    }

    #[inline]
    #[must_use]
    /// Returns the current offset into the internal [`SyscallBuffer`]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    #[inline]
    #[must_use]
    /**
    Returns the reusable kernel I/O buffer backing batched directory reads.

    This exposes the internal [`SyscallBuffer`] used by `getdents`/`getdirentries64`
    so low-level callers can inspect buffer sizing or reuse it for diagnostics.
    The buffer remains owned by the iterator and is mutated whenever a new batch
    of directory entries is fetched.
    */
    pub const fn syscall_buffer(&self) -> &SyscallBuffer {
        &self.syscall_buffer
    }

    #[inline]
    /**
     Refills the internal directory-entry buffer using the platform directory syscall.

     On Linux-like targets this dispatches to `getdents`, while on macOS and
     FreeBSD it calls `getdirentries64` and updates the internal base pointer
     required by that API.

     # Returns

     Returns the raw syscall result as a signed byte count:
     - positive: the number of bytes written into the buffer
     - `0`: end of directory stream
     - negative: the syscall failed

     This method does not interpret the returned entries; higher-level iteration
     code is responsible for consuming the buffer and handling end-of-stream.
    */
    pub fn getdents(&mut self) -> isize {
        #[cfg(not(any(target_os = "macos", target_os = "freebsd")))]
        {
            self.syscall_buffer.getdents(&self.fd)
        }
        #[cfg(any(target_os = "macos", target_os = "freebsd"))]
        {
            //SAFETY: passing a valid buffer to an open file descriptor and base pointer
            unsafe {
                self.syscall_buffer
                    .getdirentries64(&self.fd, &mut self.base_pointer)
            }
        }
    }

    #[inline]
    #[must_use]
    /// Returns the amount of bytes left in the buffer
    pub const fn remaining_bytes(&self) -> usize {
        self.remaining_bytes
    }
    /// A constant representing the maximum size of the internal Stack based buffer on this platform
    /// Differs per platform and in debug/release! Do not rely on this except if you're doing pointer arithmetic.
    pub const BUFFER_SIZE: usize = SyscallBuffer::BUFFER_SIZE;

    #[inline]
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_ptr_alignment)]
    #[allow(unfulfilled_lint_expectations)] //For platform variants with EOF trick.
    #[allow(clippy::missing_assert_message)] // for cleaner code.
    pub(crate) fn are_more_entries_remaining(&mut self) -> bool {
        // Early return if we've already reached end of stream

        if self.end_of_stream {
            return false;
        }

        #[cfg(has_eof_trick)]
        {
            // If using the EOF trick, initialise the last 4 bytes of the buffer with 0,
            // this means that we detect when the kernel writes it's EOF flags
            // SAFETY: Buffer is aligned to 8 bytes->aligned to 4, the write is in bounds by construction
            // so alignment met+accessing valid(but uinitialised) memory.
            unsafe {
                self.syscall_buffer
                    .as_mut_ptr()
                    .byte_add(Self::BUFFER_SIZE - 4)
                    .cast::<u32>()
                    .write(0)
            }
        }

        // Get the syscall return amount in bytes
        let remaining_bytes = self.getdents();

        const { assert!(Self::BUFFER_SIZE.is_multiple_of(8)) };
        debug_assert!(self.syscall_buffer.as_ptr().cast::<u64>().is_aligned());

        let is_more_remaining = remaining_bytes.is_positive();
        // Only macOS has this optimisation, the other BSD's do not
        #[cfg(has_eof_trick)] // Check at build time for the optimisation
        {
            // SAFETY: Buffer is already initialised by the kernel
            // The kernel writes the WHOLE of the buffer passed to `getdirentries`
            //(also it's to u8, which has no restrictions on alignment)
            //https://github.com/apple/darwin-xnu/blob/main/bsd/sys/dirent.h
            // The last bytes-4 is set to 1 to act as a sentinel to mark EOF(this was a PAIN to find out)
            // It always marks the end of the buffer regardless if EOF or not.
            // We can additionally deduce that readdir also uses the early EOF trick (closed source implementation)
            // https://github.com/apple-oss-distributions/Libc/blob/899a3b2d52d95d75e05fb286a5e64975ec3de757/gen/FreeBSD/opendir.c#L373-L392
            // As this is ~5 years old, we can safely assume that all kernels have this capability, this is the best we'll get
            self.end_of_stream =
            // SAFETY: the fundamentally buffer is always aligned to a multiple of 8, which means it's always aligned for 4 byte->u32 access
            unsafe { self.buffer_add(Self::BUFFER_SIZE - 4).cast::<u32>().read() == 1 }
        }

        #[cfg(not(has_eof_trick))]
        {
            self.end_of_stream = !is_more_remaining // returned bytes=0
        }

        /*
        Example of syscall differences( also note the lack of fstatfs64 and semwait signal!)

           λ   sudo dtruss -c fd -HI . ~ 2>&1 | tail -n 15

           sysctl                                         12
           ulock_wait2                                    12
           mmap                                           13
           ulock_wake                                     14
           munmap                                         17
           mprotect                                       26
           sigaltstack                                    30
           write                                         156
           madvise                                       196
           close_nocancel                               1898
           fstatfs64                                    1899
           open_nocancel                                1903
           getdirentries64                              1920
           __semwait_signal                            11184


           λ   sudo dtruss -c fdf -HI . ~ 2>&1 | tail -n 15

           munmap                                          7
           bsdthread_create                                8
           stat64                                          8
           thread_selfid                                   9
           close                                          10
           sysctl                                         11
           mmap                                           15
           sigaltstack                                    18
           mprotect                                       25
           write                                          32
           madvise                                       175
           close_nocancel                               2562
           open                                         2578
           getdirentries64                              2606
                   */

        // Branchless check
        self.remaining_bytes = remaining_bytes.cast_unsigned() * usize::from(is_more_remaining);

        self.offset = 0;

        // Return true only if we successfully read non-zero bytes
        is_more_remaining
    }

    /**
        Advances the iterator to the next directory entry in the buffer and returns a pointer to it.

        This function processes the internal buffer filled by `getdirentries(64)` system calls, interpreting
        the data at the current offset as a `dirent64` structure. After reading an entry, the internal
        offset is advanced by the entry's record length (`d_reclen`), positioning the iterator for
        the next subsequent call.

        IMPORTANT: This function returns ALL directory entries, including "." and ".." entries.
        Filtering of these entries should be handled by the caller if desired.

        # Returns
        - `Some(Unique<dirent64>)` when a valid directory entry is available
        - `None` when the buffer is exhausted and no more entries can be read

        # Behavior
        The function performs the following steps:
        1. Checks if unread data remains in the internal buffer
        2. Casts the current buffer position to a `dirent64` pointer
        3. Extracts the entry's record length to advance the internal offset
        4. Returns a non-null pointer wrapped in `Some`, or `None` at buffer end
    */
    #[inline]
    #[allow(clippy::cast_ptr_alignment)]
    pub fn get_next_entry(&mut self) -> Option<Unique<dirent64>> {
        while self.offset >= self.remaining_bytes {
            if !self.are_more_entries_remaining() {
                return None;
            }
        }
        // We have data in buffer, get next entry
        // SAFETY: the buffer is not empty and therefore has remaining bytes to be read
        let drnt = unsafe { self.buffer_add(self.offset).cast::<dirent64>() };

        // Quick sanity checks for debug builds (alignment check+nullcheck)
        debug_assert!(!drnt.is_null(), "dirent is null in get next entry!");
        debug_assert!(drnt.is_aligned(), "the dirent is malformed"); //not aligned to 8 bytes
        // SAFETY: dirent is not null so field access is safe
        self.offset += unsafe { access_dirent!(drnt, d_reclen) };
        // increment the offset by the size of the dirent structure (reclen=size of dirent struct in bytes)
        // SAFETY: dirent is not null
        unsafe { Some(Unique::new_unchecked(drnt)) }
    }

    #[inline]
    pub(crate) fn new(dir: &DirEntry) -> Result<Self> {
        let fd = dir.open()?; //getting the file descriptor
        debug_assert!(fd.is_open(), "We expect it to always be open");

        let (path_buffer, path_len) = Self::init_from_path(dir);

        Ok(Self {
            fd,
            syscall_buffer: SyscallBuffer::new(),
            path_buffer,
            file_name_index: path_len,
            parent_depth: dir.depth,
            offset: 0,
            remaining_bytes: 0,
            end_of_stream: false,
            #[cfg(any(target_os = "macos", target_os = "freebsd"))]
            base_pointer: 0,
        })
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "freebsd",
    target_os = "macos"
))]
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
        #[cfg(not(debug_assertions))]
        unsafe {
            libc::close(self.fd.0)
        };
        // SAFETY: As above
        #[cfg(debug_assertions)]
        unsafe {
            assert!(libc::close(self.fd.0) == 0, "fd was not closed in getdents")
        }
    }
}

// Cheap macro to avoid duplicate code maintenance. (Keep the documentation continuous)
macro_rules! impl_iterator_public_methods {
    ($type:ty) => {
        impl Iterator for $type {
            type Item = $crate::fs::DirEntry;

            #[inline]
            fn next(&mut self) -> Option<Self::Item> {
                while let Some(drnt) = self.get_next_entry() {
                    skip_dot_or_dot_dot_entries!(drnt.as_ptr(), continue);
                    // this just skips dot entries in a really efficient manner(avoids strlen) by checking dtype first on most OS'es
                    return Some(self.construct_direntry(drnt));
                }
                None // signal end
            }
        }

        impl $type {
            /**
            Returns the file descriptor for this directory.

            Useful for operations that need the raw directory FD.

            ISSUE: this file descriptor is only closed by the iterator due to current limitations
            */
            #[inline]
            #[must_use]
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
             * `drnt` - A `Unique` pointer to a valid `dirent64` structure

            */
            pub fn construct_direntry(
                &mut self,
                drnt: $crate::Unique<$crate::dirent64>,
            ) -> $crate::fs::DirEntry {
                self.construct_entry(drnt)
            }
        }
    };
}

// Simple repetition avoider for private trait
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

// Common to all platforms
impl_iterator_public_methods!(ReadDir);
impl_dirent_constructor!(ReadDir);

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "freebsd",
    target_os = "macos"
))]
impl_iterator_public_methods!(GetDents);
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "freebsd",
    target_os = "macos"
))]
impl_dirent_constructor!(GetDents);
