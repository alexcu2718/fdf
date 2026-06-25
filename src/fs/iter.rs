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
use core::mem::MaybeUninit;
use core::ptr::NonNull;
use libc::{AT_SYMLINK_NOFOLLOW, DIR, closedir, fstatat};
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
    pub(crate) path_buffer: Vec<MaybeUninit<u8>>,
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

    /// Constructs a `ReadDir` from a pre-opened file descriptor, skipping the `open()` call.
    ///
    /// Used when the caller already holds an fd obtained via `openat`, avoiding a second
    /// full-path resolution for the child directory.
    #[inline]
    pub(crate) fn from_fd(fd: FileDes, dir_path: &DirEntry) -> Self {
        let (path_buffer, file_name_index) = Self::init_from_path(dir_path);
        // SAFETY: caller provides a valid directory fd; fdopendir takes ownership.
        let dir = unsafe { NonNull::new_unchecked(libc::fdopendir(fd.0)) };
        debug_assert!(fd.is_open(), "We expect it to be open");
        Self {
            dir,
            path_buffer,
            file_name_index,
            parent_depth: dir_path.depth,
            fd,
        }
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
pub(crate) trait DirentConstructor {
    /// Returns a mutable reference to the path buffer used for constructing full paths
    fn path_buffer(&mut self) -> &mut Vec<MaybeUninit<u8>>;
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
    /// Returns total allocated capacity of the buffer.
    fn total_capacity(&self) -> usize;

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
    fn init_from_path(path: &[u8]) -> (Vec<MaybeUninit<u8>>, usize) {
        use core::slice;
        let mut dirlen = path.len();
        let needs_slash = usize::from(path != b"/");

        // Fast-path filename capacity (+NUL is included in `name_len` during append).
        // Longer names take the cold slow-path reserve in `construct_path`.
        // Most filepaths will never be longer than this. In the odd-case they are, it's really rare
        // with no negligible affect otherwise
        const FAST_PATH_DIRENT_LENGTH: usize = 256;
        let total_capacity = dirlen + FAST_PATH_DIRENT_LENGTH + needs_slash;

        let mut buffer: Vec<MaybeUninit<u8>> = Vec::with_capacity(total_capacity);

        // SAFETY: we immediately write the bytes we read from and later overwrite filename bytes.
        unsafe { buffer.set_len(total_capacity) };

        // SAFETY:
        // - buffer has length `total_capacity`, so first `dirlen` bytes are in-bounds
        // - cast to u8 is valid because MaybeUninit<u8> has same layout/alignment as u8 (trivial)
        let dst_prefix: &mut [u8] =
            unsafe { slice::from_raw_parts_mut(buffer.as_mut_ptr().cast(), dirlen) };

        dst_prefix.copy_from_slice(path);

        // SAFETY: In-bounds because total_capacity = dirlen + FAST_PATH_DIRENT_LENGTH + 1
        unsafe { buffer.get_unchecked_mut(dirlen).write(b'/') };

        dirlen += needs_slash;
        (buffer, dirlen)
    }

    #[cold]
    #[inline(never)]
    fn reserve_for_long_name(&mut self, required_len: usize) {
        let path_buffer = self.path_buffer();
        let current_len = path_buffer.len();
        path_buffer.reserve_exact(required_len - current_len);
        // SAFETY: we reserved enough capacity and bytes in the extended range
        // are immediately written by `copy_to_nonoverlapping`.
        unsafe { path_buffer.set_len(required_len) };
    }

    /**
    Constructs a full path by appending the directory entry name to the base path

    returns the full path, inode,`FileType` (not abstracted into types bc of  internal use only)
    */
    #[inline]
    #[rustfmt::skip]
    #[expect(clippy::indexing_slicing, reason = "debug build only")]
    fn construct_path(&mut self, drnt: Unique<dirent64>) -> (&CStr, u64, FileType) {
        use core::slice::from_raw_parts_mut;
        let d_name:&CStr = drnt.d_name_cstr();
        let d_ino = drnt.d_ino(); // Returns 0 if d_ino isn't defined on your system
        let name_len=d_name.count_bytes()+1; // strlen(x)+1 but the length is already computed by the slice.
        // Add 1 to include the null terminator


        // if d_type==`DT_UNKNOWN`  then make an fstat at call to determine
        #[cfg(has_d_type)]
        let file_type: FileType = match FileType::from_dtype(drnt.d_type()) {
            FileType::Unknown => stat_syscall!(
                fstatat,
                self.file_descriptor().0, //borrow before mutably borrowing the path buffer
                d_name.as_ptr().cast(), //cast into i8 (depending on architecture, pointers are either i8/u8)
                AT_SYMLINK_NOFOLLOW, // dont follow, to keep same semantics as readdir/getdents
                DTYPE
            ),
            not_unknown => not_unknown, //if not unknown, skip the syscall (THIS IS A MASSIVE PERF WIN)
        };

        #[cfg(not(has_d_type))] // Have to make a syscall on these systems alas
        let file_type = stat_syscall!(
            fstatat,
            self.file_descriptor().0, //borrow before mutably borrowing the path buffer
            d_name.as_ptr().cast(), //cast into i8 (depending on architecture, pointers are either i8/u8)
            AT_SYMLINK_NOFOLLOW, // dont follow, to keep same semantics as readdir/getdents
            DTYPE
        );

        let base_len = self.file_index();
        let total_capacity = self.total_capacity();
        let required_len = base_len + name_len;

        if required_len > total_capacity {
            self.reserve_for_long_name(required_len); // unlikely branch.
        }
        let path_ptr:*mut u8 = self.path_buffer().as_mut_ptr().cast();

        // SAFETY: path_buffer len is at least required_len here.
        // MaybeUninit<u8> has same layout as u8.
        let bytes: &mut [u8] = unsafe {from_raw_parts_mut(path_ptr, required_len)};




        // use a memcpy (under the hood)
        // SAFETY: always in bounds
        unsafe {bytes.get_unchecked_mut(base_len..required_len).copy_from_slice(d_name.to_bytes_with_nul())};


        // SAFETY: we just ensured [0..required_len] is initialised and NUL-terminated.
        let full_path = unsafe { CStr::from_bytes_with_nul_unchecked(&bytes[..required_len]) };

        // BY doing it this way, we avoid calling strlen, which as the path increases in size, will end up taking a hefty toll on CPU calculations
        // SAFETY:
        debug_assert!(unsafe{CStr::from_ptr(bytes.as_ptr().cast())}==full_path,"testing for interior nulls");

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
    pub(crate) path_buffer: Vec<MaybeUninit<u8>>,
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
    pub(crate) const unsafe fn get_next_pointer(&self) -> Unique<dirent64> {
        debug_assert!(
            self.offset.is_multiple_of(8),
            "offset should always be multiple of 8"
        );
        // SAFETY:  internal use only, the `offset` parameter is always within bounds of the buffer.
        unsafe {
            Unique::new_unchecked(
                self.syscall_buffer
                    .as_ptr()
                    .byte_add(self.offset)
                    .cast::<dirent64>(),
            )
        }
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
    Returns the mutable reusable kernel I/O buffer backing batched directory reads.

    This exposes the internal [`SyscallBuffer`] used by `getdents`/`getdirentries64`
    so low-level callers can inspect buffer sizing or reuse it for diagnostics.
    The buffer remains owned by the iterator and is mutated whenever a new batch
    of directory entries is fetched.
    */
    pub const fn syscall_buffer(&mut self) -> &mut SyscallBuffer {
        &mut self.syscall_buffer
    }

    /// Convenience function to safe verbosity(+safety)
    ///
    /// By constructing a NonNull, we car
    #[inline]
    #[must_use]
    #[cfg(has_eof_trick)]
    pub(crate) const fn syscall_buffer_ptr(&mut self) -> NonNull<u64> {
        // SAFETY: never null
        unsafe { NonNull::new_unchecked(self.syscall_buffer().as_mut_ptr()) }
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

    #[inline]
    #[must_use]
    /// Returns a boolean indicating whether the stream is exhausted
    pub const fn is_end_of_stream(&self) -> bool {
        self.end_of_stream
    }

    /// A constant representing the maximum size of the internal Stack based buffer on this platform
    /// Differs per platform and in debug/release! Do not rely on this except if you're doing pointer arithmetic.
    pub const BUFFER_SIZE: usize = SyscallBuffer::BUFFER_SIZE;

    #[inline]
    #[allow(clippy::missing_assert_message)] // for cleaner code.
    pub(crate) fn are_more_entries_remaining(&mut self) -> bool {
        // Early return if we've already reached end of stream

        if self.is_end_of_stream() {
            return false;
        }

        const { assert!(Self::BUFFER_SIZE.is_multiple_of(8), "proving alignment") };

        #[cfg(has_eof_trick)]
        #[rustfmt::skip]
        /*
         Create a ptr to the last four bytes of the buffer, use this to detect sentinel changes with EOF behaviour (macOS exclusive).
         In doing the `getdirentries64` syscalls, we zero the last four bytes, so they're guaranteed initialised.
         If this marker changes, the kernel has indicated EOF, the buffer is never filled up the the maximum
         (I've done some rudimentary println and syscall tracing of the buffer, it always leaves a reserved space, probably some reference exists but too lazy currently.)
         Alignment of 8 => Alignment of 4 guaranteed invariant. */
        // SAFETY: see above
        let last_four_bytes: &mut MaybeUninit<u32> = unsafe {
            self.syscall_buffer_ptr().byte_add(Self::BUFFER_SIZE - 4)
            .cast::<MaybeUninit<u32>>().as_mut()
        };
        // TODO replace with this once rust 1.95 on all CI platforms https://doc.rust-lang.org/src/core/ptr/mut_ptr.rs.html#618
        // Basically rust 'knows' under the hood that creating an (aligned) reference to uninit memory that is
        // *always* in bounds, so hence why it skips the panic branch.

        #[cfg(has_eof_trick)]
        // If using the EOF trick, initialise the last 4 bytes of the buffer with 0,
        // this means that we detect when the kernel writes it's EOF flags
        // Write a 0 to initialise the memory, we check if this changes to 1 after the syscall
        let last_four_bytes_init = last_four_bytes.write(0);

        // Get the syscall return amount in bytes
        let remaining_bytes = self.getdents();

        let is_more_remaining = remaining_bytes.is_positive();
        // Only macOS has this optimisation, the other platforms do not, if macos does not have this, default to checking for 0 ret value

        // Check the last four bytes for the marker
        // Also the XNU kernel never fills buffer up to maximum size, it always has a flags section towards the end.
        //https://github.com/apple/darwin-xnu/blob/main/bsd/sys/dirent.h
        // We can additionally deduce that readdir also uses the early EOF trick (closed source implementation)
        // https://github.com/apple-oss-distributions/Libc/blob/899a3b2d52d95d75e05fb286a5e64975ec3de757/gen/FreeBSD/opendir.c#L373-L392
        // As this has existed for decades, it's relatively stable, any ABI breaks are unlikely since we're on 64bit for good.
        #[cfg(has_eof_trick)]
        {
            self.end_of_stream = *last_four_bytes_init == 1 || !is_more_remaining
        }
        // Check at build time for the optimisation
        // check if the syscall returns 0 too, the latter branch should almost never be true on supported system

        // returned bytes=0
        #[cfg(not(has_eof_trick))]
        {
            self.end_of_stream = !is_more_remaining
        }

        /*
        Example of syscall differences( also note the lack of fstatfs64 and semwait signal!)
        macOS is only virtualised via qemu, I get some wacky results, I don't know why
        the open calls differ EVERY invocation, maybe something to do with macs anti malware or strange apple-ism?
           λ   sudo dtruss -c fd -HI . ~ 2>&1 | tail -n 15
           write                                         156
           madvise                                       196
           close_nocancel                               1898
           fstatfs64                                    1899
           open_nocancel                                1903
           getdirentries64                              1920
           __semwait_signal                            11184
           λ   sudo dtruss -c fdf -HI . ~ 2>&1 | tail -n 15
           write                                          32
           madvise                                       175
           close_nocancel                               2562
           open                                         2578
           getdirentries64                              2606 */
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
    #[allow(clippy::missing_assert_message)]
    pub fn get_next_entry(&mut self) -> Option<Unique<dirent64>> {
        while self.offset >= self.remaining_bytes {
            if !self.are_more_entries_remaining() {
                return None;
            }
        }
        debug_assert!(self.offset.is_multiple_of(8), "Must be a multiple of 8");
        const { assert!(align_of::<u64>() == align_of::<dirent64>()) };
        // We have data in buffer, get next entry
        // SAFETY: the buffer is not empty and therefore has remaining bytes to be read and is properly aligned
        let drnt = unsafe { self.get_next_pointer() };

        // Increment the offset by the reclen to get to the next `dirent64` pointer
        self.offset += drnt.d_reclen();
        // increment the offset by the size of the dirent structure (reclen=size of dirent struct in bytes)
        Some(drnt)
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
            #[cfg(any(target_os = "macos", target_os = "freebsd"))]
            base_pointer: 0,
        })
    }

    /// Constructs a `GetDents` from a pre-opened file descriptor, skipping the `open()` call.
    ///
    /// Used when the caller already holds an fd obtained via `openat`, avoiding a second
    /// full-path resolution for the child directory.
    /// Used internally only due to non-enforceable invariants
    #[inline]
    pub(crate) fn from_fd(fd: FileDes, dir: &DirEntry) -> Self {
        debug_assert!(fd.is_open(), "We expect it to always be open");
        let (path_buffer, file_name_index) = Self::init_from_path(dir);
        Self {
            fd,
            syscall_buffer: SyscallBuffer::new(),
            path_buffer,
            file_name_index,
            parent_depth: dir.depth,
            offset: 0,
            remaining_bytes: 0,
            end_of_stream: false,
            #[cfg(any(target_os = "macos", target_os = "freebsd"))]
            base_pointer: 0,
        }
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
            fn path_buffer(&mut self) -> &mut Vec<core::mem::MaybeUninit<u8>> {
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
            #[inline]
            fn total_capacity(&self) -> usize {
                self.path_buffer.len()
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
