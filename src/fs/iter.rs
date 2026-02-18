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
use crate::fs::{DirEntry, FileType};
use crate::fs::{FileDes, Result};
use crate::util::dirent_name_length;
use crate::{dirent64, readdir64};
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
        let (path_buffer, file_name_index) = Self::init_from_path(dir_path);
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
    // TODO! Investigate how std lib manages to not piss off FDsan
    //( i want to pass ownership to the FileDes BUT android complains, even though it works. weird.)
}

/**
Linux/Android/BSDspecific directory iterator using the `getdents` system call.

Provides more efficient directory traversal than `readdir` for large directories

Unlike some directory iteration methods, this does not implicitly call `stat`
on each entry unless required by unusual filesystem behaviour.
*/
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris"
))]
pub struct GetDents {
    /// File descriptor of the open directory, wrapped for automatic resource management
    pub(crate) fd: FileDes,
    /// Kernel buffer for batch reading directory entries via system call I/O
    /// Approximately 32kB in size, optimised for typical directory traversal
    pub(crate) syscall_buffer: SyscallBuffer,
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

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris"
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

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris"
))]
impl GetDents {
    /**
    Returns the number of unprocessed bytes remaining in the current kernel buffer.

    This indicates how much data is still available to be processed before needing
    to perform another `getdents(64)` system call. When this returns 0, the buffer
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
    #[must_use]
    pub const fn remaining_bytes(&self) -> usize {
        self.remaining_bytes
    }

    #[inline]
    pub(crate) fn are_more_entries_remaining(&mut self) -> bool {
        // Early return if we've already reached end of stream
        if self.end_of_stream {
            return false;
        }

        // Read directory entries, ignoring negative error codes(same as readdir semantics)
        let remaining_bytes = self.syscall_buffer.getdents(&self.fd);

        let has_bytes_remaining = remaining_bytes.is_positive();
        // Cast the boolean to 0/1  (because 0 * x=0, trivially), keeping only positive results(avoid branching)
        self.remaining_bytes = usize::from(has_bytes_remaining) * remaining_bytes.cast_unsigned();

        self.end_of_stream = !has_bytes_remaining;

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
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
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
        Ok(Self {
            fd,
            syscall_buffer: SyscallBuffer::new(),
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
    #[allow(clippy::cast_ptr_alignment)]
    pub fn get_next_entry(&mut self) -> Option<NonNull<dirent64>> {
        while self.offset >= self.remaining_bytes {
            if !self.are_more_entries_remaining() {
                return None;
            }
        }

        // We have data in buffer, get next entry
        // SAFETY: the buffer is not empty and therefore has remaining bytes to be read
        let drnt = unsafe {
            self.syscall_buffer
                .as_ptr()
                .add(self.offset)
                .cast::<dirent64>()
        };

        // Quick sanity checks for debug builds (alignment check+nullcheck)
        debug_assert!(!drnt.is_null(), "dirent is null in get next entry!");
        debug_assert!(drnt.is_aligned(), "the dirent is malformed"); //not aligned to 8 bytes
        // SAFETY: dirent is not null so field access is safe
        self.offset += unsafe { access_dirent!(drnt, d_reclen) };
        // increment the offset by the size of the dirent structure (reclen=size of dirent struct in bytes)
        // SAFETY: dirent is not null (need to cast to mut for `NonNull` sadly.)
        unsafe { Some(NonNull::new_unchecked(drnt.cast_mut())) }
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
    /// Constructs a `DirEntry` from a raw directory entry pointer
    unsafe fn construct_entry(&mut self, drnt: *const dirent64) -> DirEntry {
        debug_assert!(!drnt.is_null(), "drnt should never be null!");
        // SAFETY: The `drnt` must not be null(by precondition)
        let (cstrpath, inode, file_type): (&CStr, u64, FileType) =
            unsafe { self.construct_path(drnt) };

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

        let is_root = path == b"/";

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
            *path_buffer.as_mut_ptr().add(base_len) = b'/' //this doesnt matter for non directories, since we're overwriting it anyway
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
    unsafe fn construct_path(&mut self, drnt: *const dirent64) -> (&CStr, u64, FileType) {
        debug_assert!(!drnt.is_null(), "drnt is null in construct path!");
        // SAFETY: The `drnt` must not be null (checked before using)
        let d_name: *const u8 = unsafe { access_dirent!(drnt, d_name) };
        #[cfg(has_d_ino)]
        // SAFETY: same as above
        let d_ino: u64 = unsafe { access_dirent!(drnt, d_ino) };
        #[cfg(not(has_d_ino))]
        let d_ino: u64 = 0;
        // SAFETY: Same as above^ (Add 1 to include the null terminator)
        let name_len = unsafe { dirent_name_length(drnt) + 1 }; //technically should be a u16 but we need it for indexing :(

        // if d_type==`DT_UNKNOWN`  then make an fstat at call to determine
        #[cfg(has_d_type)]
        let file_type: FileType =
            // SAFETY: as above.
            match FileType::from_dtype(unsafe { access_dirent!(drnt, d_type) }) {
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

        let base_len = self.file_index();
        // Get the portion of the buffer that goes past the last slash
        // SAFETY: The `base_len` is guaranteed to be a valid index into `path_buffer`
        let buffer: &mut [u8] = unsafe { self.path_buffer().get_unchecked_mut(base_len..) };

        // SAFETY: `d_name` and `buffer` don't overlap (different memory regions)
        // - Both pointers are properly aligned for byte copying
        // - `name_len` is within `buffer` bounds
        // Copy the name into the final portion
        unsafe { d_name.copy_to_nonoverlapping(buffer.as_mut_ptr(), name_len) };
        // SAFETY: the buffer is guaranteed null terminated and we're accessing in bounds
        let full_path = unsafe {
            CStr::from_bytes_with_nul_unchecked(
                self.path_buffer().get_unchecked(..base_len + name_len),
            )
        }; //truncate the buffer to the first null terminator of the full path

        (full_path, d_ino, file_type)
    }
}

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
/**
macOS/freeBSD directory iterator using the `getdirentries` system call.

 Provides more efficient directory traversal than `readdir` for large directories

 Unlike some directory iteration methods, this does not implicitly call `stat`
 on each entry unless required by unusual filesystem behaviour.
*/
pub struct GetDirEntries {
    /// File descriptor of the open directory, wrapped for automatic resource management
    pub(crate) fd: FileDes,
    /// Kernel buffer for batch reading directory entries via system call I/O
    /// 8192 bytes, matching macos readdir semantics) in size, optimised for typical directory traversal
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
    /// The base pointer for the getdirentries call
    pub(crate) base_pointer: i64,
}

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
impl GetDirEntries {
    #[inline]
    #[allow(clippy::cast_sign_loss)]
    pub(crate) fn are_more_entries_remaining(&mut self) -> bool {
        // Early return if we've already reached end of stream
        if self.end_of_stream {
            return false;
        }

        //SAFETY: passing a valid buffer to an open file descriptor.
        let remaining_bytes = unsafe {
            self.syscall_buffer
                .getdirentries64(&self.fd, &raw mut self.base_pointer)
        };
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
            // (Or at least I cant find it anywhere!)
            // https://github.com/apple-oss-distributions/Libc/blob/899a3b2d52d95d75e05fb286a5e64975ec3de757/gen/FreeBSD/opendir.c#L373-L392
            // As this is ~5 years old, we can safely assume that all kernels have this capability

            self.end_of_stream =
            // SAFETY: AS ABOVE
            unsafe { self.syscall_buffer.as_ptr().add(SyscallBuffer::BUFFER_SIZE - 4).read() == 1 };
        }
        #[cfg(not(has_eof_trick))]
        {
            self.end_of_stream = !is_more_remaining // returned bytes=0
        }

        /*




        Example of syscall differences( also note the lack of fstatfs64!)



            /tmp/fdf_test getdirentries !8 ❯ sudo dtruss -c fd -HI . ~ 2>&1  | tail                              ✘ 0|INT 4s alexc@alexcs-iMac 01:03:12

            psynch_mutexdrop                              165
            psynch_mutexwait                              165
            __semwait_signal                              834
            madvise                                      1337
            close_nocancel                               5050
            fstatfs64                                    5054
            open_nocancel                                5054
            getdirentries64                              5399
            write                                        6267

            /tmp/fdf_test getdirentries !8 ❯ sudo dtruss -c ./target/release/fdf -HI . ~ 2>&1 | tail                    47s alexc@alexcs-iMac 01:04:06

            psynch_mutexdrop                               91
            psynch_mutexwait                               91
            stat64                                        138
            psynch_cvsignal                               142
            psynch_cvwait                                 153
            write                                        2414
            close_nocancel                               3538
            open                                         3543
            getdirentries64                              3545



        */

        // Branchless check
        self.remaining_bytes = remaining_bytes.cast_unsigned() * usize::from(is_more_remaining);

        self.offset = 0;

        // Return true only if we successfully read non-zero bytes
        is_more_remaining
    }
    #[inline]
    #[allow(clippy::cast_ptr_alignment)]
    pub fn get_next_entry(&mut self) -> Option<NonNull<dirent64>> {
        while self.offset >= self.remaining_bytes {
            if !self.are_more_entries_remaining() {
                return None;
            }
        }
        // We have data in buffer, get next entry
        // SAFETY: the buffer is not empty and therefore has remaining bytes to be read
        let drnt = unsafe {
            self.syscall_buffer
                .as_ptr()
                .add(self.offset)
                .cast::<dirent64>()
        };

        // Quick sanity checks for debug builds (alignment check+nullcheck)
        debug_assert!(!drnt.is_null(), "dirent is null in get next entry!");
        debug_assert!(drnt.is_aligned(), "the dirent is malformed"); //not aligned to 8 bytes
        // SAFETY: dirent is not null so field access is safe
        self.offset += unsafe { access_dirent!(drnt, d_reclen) };
        // increment the offset by the size of the dirent structure (reclen=size of dirent struct in bytes)
        // SAFETY: dirent is not null
        unsafe { Some(NonNull::new_unchecked(drnt.cast_mut())) }
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
            base_pointer: 0,
        })
    }
}

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
impl Drop for GetDirEntries {
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
            /// Returns whether this opened file descriptor has a gitignore file
            /// If so, return the size of the file in bytes(so we can allocate appropriate memory)
            #[allow(clippy::cast_sign_loss)]
            #[allow(clippy::cast_possible_truncation)]
            pub fn has_gitignore(&self) -> Option<core::num::NonZeroUsize> {
                const IGNORE: &core::ffi::CStr = c".gitignore";
                let mut stat_buf = core::mem::MaybeUninit::<libc::stat>::uninit();
                // SAFETY: trivial(always passing a null terminated string)
                let statted = unsafe {
                    libc::fstatat(
                        self.dirfd().0,
                        IGNORE.as_ptr(),
                        stat_buf.as_mut_ptr(),
                        libc::AT_SYMLINK_NOFOLLOW,
                    ) == 0
                };
                if !statted {
                    return None;
                }

                // SAFETY: `fstatat` succeeded, so `stat_buf` is initialised.
                let stat = unsafe { stat_buf.assume_init() };
                let mode = stat.st_mode;
                let is_regular = (mode & libc::S_IFMT) == libc::S_IFREG;
                let is_user_readable = (mode & libc::S_IRUSR) != 0;

                if !(is_regular && is_user_readable) {
                    return None;
                }
                // Return some only if the file is not empty, no point parsing an empty gitignore!
                // `st_size` is i64/u64/whatever depending on platform, it will NEVER be negative
                // so casting it is fine.

                core::num::NonZeroUsize::new(stat.st_size as _)
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
    target_os = "solaris"
))]
impl_iterator_public_methods!(GetDents);
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris"
))]
impl_dirent_constructor!(GetDents);

// Macos/FreeBSD's only(?)
#[cfg(any(target_os = "macos", target_os = "freebsd"))]
impl_iterator_public_methods!(GetDirEntries);
#[cfg(any(target_os = "macos", target_os = "freebsd"))]
impl_dirent_constructor!(GetDirEntries);
