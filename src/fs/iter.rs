#[cfg(has_faster_readdir)]
use crate::fs::SyscallBuffer;
use crate::fs::{DirEntry, FileType, OwnedDir, Result};
use crate::{Unique, dirent64};
use core::cell::Cell;
use core::ffi::CStr;
use core::hint::assert_unchecked;
use core::mem::MaybeUninit;
use libc::{AT_SYMLINK_NOFOLLOW, fstatat};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};

/**
 POSIX-compliant directory iterator using libc's readdir

 This iterator traverses directory entries using the standard POSIX directory
 reading API. It automatically skips "." and ".." entries and provides
 a safe Rust interface over the underlying C library functions.

*/
#[derive(Debug)]
pub struct ReadDir<'filedes> {
    /// Directory pointer from libc's `opendir()` wrapped in a new type with custom destructor.
    fd: OwnedDir<'filedes>, //call it fd for macro code
    /// Bookmarked base-path state (buffer + filename index) used to construct entry paths
    path: PathBuffer,
    /// Depth of this directory relative to traversal root
    parent_depth: u32,
}

impl ReadDir<'_> {
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

    # Safety
    The directory stream is always valid and open for the lifetime of `self` (the destructor closes it).
    The returned pointer is only valid until the next call to `get_next_entry`; it must be
    consumed (e.g. via `construct_entry`) before advancing the stream again.
    */
    #[inline]
    pub unsafe fn get_next_entry(&mut self) -> Option<Unique<dirent64>> {
        self.fd.next_pointer()
    }

    #[inline]
    pub(crate) fn new(dir_path: &DirEntry) -> Result<Self> {
        let fd = OwnedDir::from_owned_fd(dir_path.open()?)?; //read file descriptor
        // Mutate the buffer to contain the full path, then add a null terminator and record the new length
        // We use this length to index to get the filename (store full path -> index to get filename)
        let path = PathBuffer::new(dir_path);

        Ok(Self {
            fd,
            path,
            parent_depth: dir_path.depth, //inherit depth
        })
    }

    /// Constructs a `ReadDir` from a pre-opened file descriptor, skipping the `open()` call.
    ///
    /// Used when the caller already holds an fd obtained via `openat`, avoiding a second
    /// full-path resolution for the child directory.
    #[inline]
    pub(crate) fn from_fd(dfd: OwnedFd, dir_path: &DirEntry) -> Result<Self> {
        let fd = OwnedDir::from_owned_fd(dfd)?;
        let path = PathBuffer::new(dir_path);
        // fdopendir takes ownership.
        Ok(Self {
            fd,
            path,
            parent_depth: dir_path.depth,
        })
    }
}

/**
 Bookmarked path-construction state shared by every directory iterator.

 Combines the base-path buffer with the index marking where filenames start,
 so the two always stay in sync.
*/
#[derive(Debug)]
struct PathBuffer {
    buffer: Vec<MaybeUninit<u8>>,
    /// Index into `buffer` where filenames start (avoids recalculating)
    file_index: usize,
}

impl PathBuffer {
    #[inline]
    #[expect(clippy::indexing_slicing, reason = "panic free")]
    fn new(path: &[u8]) -> Self {
        let needs_slash = usize::from(path != b"/");
        /*
        Fast-path filename capacity (+NUL is included in the filename during append).
        Longer names take the cold slow-path reserve in `reserve_for_long_name`.
        Most filepaths will never be longer than this. In the odd-case they are, it's really rare
        with no negligible affect otherwise
        */
        let len = path.len();
        const FAST_PATH_DIRENT_LENGTH: usize = 256;
        let total_capacity = len + FAST_PATH_DIRENT_LENGTH + needs_slash;

        let mut buffer: Vec<MaybeUninit<u8>> = Vec::with_capacity(total_capacity);
        // SAFETY: we immediately write the bytes we read from and later overwrite filename bytes.
        // The memory remains uninitialised though but we track capacity via its len, as len==capacity.
        unsafe { buffer.set_len(total_capacity) };

        //SAFETY: always true, allow compiler to elide panics
        unsafe { assert_unchecked(buffer.len() > len) };

        // (this is internally a `memcpy`)
        buffer[..len].write_copy_of_slice(path);

        // In-bounds always
        buffer[len].write(b'/');

        let file_index = len + needs_slash;
        Self { buffer, file_index }
        /* for 'len' we only read up to len bytes, so inc the ase it's not a directory, we intentionally omit the last slash by truncation.*/
    }

    #[inline]
    /// Returns the current index in the buffer where the filename should be appended
    ///
    /// This represents the length of the base directory path before adding the current filename.
    const fn file_index(&self) -> usize {
        self.file_index
    }

    #[inline]
    /// Total allocated size of the internal vector that holds the path buffer
    fn allocated_size(&self) -> usize {
        let allocated_sz = self.buffer.len();
        debug_assert!(allocated_sz <= self.buffer.capacity(), "");
        // trivial check, this should always be true, debug invariant.
        allocated_sz
    }

    #[inline(never)]
    fn reserve_for_long_name(&mut self, required_len: usize) {
        let current_len = self.buffer.len();
        self.buffer.reserve(required_len - current_len);
        // SAFETY: we reserved+(some extra, we assume weird fs here, like reiser or openzfs ) enough capacity and bytes in the extended range
        // are immediately written by `copy_to_nonoverlapping`.
        unsafe { self.buffer.set_len(required_len) };
    }

    #[inline]
    #[expect(clippy::indexing_slicing, reason = "panic free")]
    /// Ensures the buffer can hold `required_len` bytes, growing it (cold path) if necessary,
    /// then returns the portion of the buffer truncated to that length.
    fn prepare(&mut self, required_len: usize) -> &mut [MaybeUninit<u8>] {
        if crate::util::unlikely(required_len > self.allocated_size()) {
            self.reserve_for_long_name(required_len);
        }
        // SAFETY: the branch above guarantees `buffer.len() >= required_len`.
        unsafe { assert_unchecked(required_len <= self.buffer.len()) }
        &mut self.buffer[..required_len]
    }
}

/**
  Internal trait for constructing directory entries during iteration

 This trait provides the necessary components to construct `DirEntry` objects
 from raw `dirent64` structures while maintaining path buffer state, tracking
 file name positions, and managing directory traversal depth.

*/
trait DirentConstructor: AsRawFd + AsFd {
    /// Returns a mutable reference to the bookmarked path-buffer state used to construct full paths
    fn path_buffer(&mut self) -> &mut PathBuffer;
    /// Returns the depth of the parent directory in the traversal hierarchy
    ///
    /// Depth starts at 0 for the root directory being scanned and increments for each subdirectory.
    fn parent_depth(&self) -> u32;

    #[inline]
    /// Constructs a `DirEntry` from a raw directory entry pointer
    fn construct_entry(&mut self, drnt: Unique<dirent64>) -> DirEntry {
        let (cstrpath, inode, file_type): (&CStr, u64, FileType) = self.construct_path(drnt);

        DirEntry {
            path: cstrpath.into(),
            file_type,
            inode,
            depth: self.parent_depth() + 1,
            file_name_index: self.path_buffer().file_index(),
            is_cached_traversable: Cell::new(None), // Lazy cache for traversal checks
        }
    }

    #[inline]
    /// Gets the `FileType` without following
    fn get_filetype(&self, drnt: &Unique<dirent64>) -> FileType {
        stat_syscall!(
            fstatat,
            self.as_raw_fd(),
            drnt.d_name(),
            AT_SYMLINK_NOFOLLOW, // dont follow, to keep same semantics as readdir/getdents
            DTYPE
        )
    }

    /**
    Constructs a full path by appending the directory entry name to the base path

    returns the full path, inode,`FileType`
    */
    #[inline]
    // TODO! make a temp dirent struct being fd:borrowedfd,name:&cstr,filetype:Filetype,ino:u64 etc, needs lifetime fuckery.
    #[expect(clippy::indexing_slicing, reason = "panic free")]
    fn construct_path(&mut self, drnt: Unique<dirent64>) -> (&CStr, u64, FileType) {
        // SAFETY: we just obtained this pointer and hasn't been invalidated by iterator reset
        let d_name_cstr: &[u8] = unsafe { drnt.d_name_cstr().to_bytes_with_nul() };

        debug_assert!(drnt.is_aligned(), "drnt not aligned(report this)");

        // SAFETY: as above
        let d_ino = unsafe { drnt.d_ino() }; // Returns 0 if d_ino isn't defined on your system

        //includes the null terminator
        let name_len = d_name_cstr.len(); //technically should be a u16 but we need it for indexing :(

        // if d_type==`DT_UNKNOWN`  then make an fstat at call to determine
        #[cfg(has_d_type)]
        // SAFETY: same conditions for the d/name etc
        let file_type: FileType = unsafe {
            match FileType::from_dtype(drnt.d_type()) {
                FileType::Unknown => self.get_filetype(&drnt),
                not_unknown => not_unknown, //if not unknown, skip the syscall (THIS IS A MASSIVE PERF WIN)
            }
        };

        #[cfg(not(has_d_type))] // Have to make a syscall on these systems alas
        let file_type = self.get_filetype(&drnt);

        let path_buffer = self.path_buffer();
        let base_len = path_buffer.file_index();
        let required_len = base_len + name_len;
        //SAFETY: trivial, name_len=>1 (POSIX standard) ; ALLOWS US TO ELIDE BOUNDS CHECKS
        // rustc knows this but it's unsigned overflow conditions being annoying.
        unsafe { assert_unchecked(required_len > base_len) };

        // Get the amount of the buffer we wish to initialise (grows the buffer on the cold path if required).
        let buffer = path_buffer.prepare(required_len);

        // Get the portion of the buffer that goes past the last slash, up to the name+NUL
        let name_portion = &mut buffer[base_len..];

        // Copy name into the uninit memory.
        name_portion.write_copy_of_slice(d_name_cstr);
        //^this is a memcpy.
        // SAFETY: the buffer is guaranteed null terminated(copying a null terminated string)
        // &CStr is a &[u8]/&[i8] underneath.
        let full_path = unsafe { &*((&raw const *buffer) as *const CStr) };
        //truncate the buffer to the first null terminator of the full path

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
#[cfg(has_faster_readdir)]
pub struct GetDents {
    /// The (owned) File descriptor of the open directory, closed when the iterator is dropped.
    fd: OwnedFd,
    /// Kernel buffer for batch reading directory entries via system call I/O
    /// typically using the best calculated  buffer sizes, optimised for typical directory traversal (derived from syscall tracing)
    syscall_buffer: SyscallBuffer,
    /// Bookmarked base-path state (buffer + filename index) used to construct entry paths
    /// Reused for each entry to avoid repeated memory allocation (only constructed once per dir)
    path: PathBuffer,
    /// Depth of the parent directory in the directory tree hierarchy
    /// Used to calculate depth for child entries during recursive traversal
    parent_depth: u32,
    /// Current read position within the directory entry buffer
    /// Tracks progress through the currently loaded batch of entries
    offset: usize,
    /// Number of bytes remaining to be processed in the current buffer
    /// Indicates when a new system call is needed to fetch more entries
    remaining_bytes: usize,
    /// A marker for when the `FileDes` can give no more entries
    end_of_stream: bool,
    #[cfg(any(target_os = "freebsd", target_os = "macos"))] // TODO add dragonflyBSD here eventually
    /// The base pointer for the getdirentries call
    base_pointer: libc::off_t,
}

#[cfg(has_faster_readdir)]
impl GetDents {
    #[inline]
    #[must_use]
    /// Returns the current offset into the internal (stack) buffer.
    pub const fn offset(&self) -> usize {
        self.offset
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
            self.syscall_buffer.getdents(self.fd.as_fd()) // awkward syntax to avoid borrow issues.
        }
        #[cfg(any(target_os = "macos", target_os = "freebsd"))]
        {
            //SAFETY: passing a valid buffer to an open file descriptor and base pointer
            unsafe {
                self.syscall_buffer
                    .getdirentries64(self.fd.as_fd(), &mut self.base_pointer)
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
    pub(crate) fn are_more_entries_remaining(&mut self) -> bool {
        // Early return if we've already reached end of stream

        if self.is_end_of_stream() {
            return false;
        }

        const { assert!(Self::BUFFER_SIZE.is_multiple_of(8), "proving alignment") };
        // syscall buffer is guaranteed to be aligned to 8, check the implementation.

        #[cfg(has_eof_trick)]
        /*
        Create a pointer to the last four bytes of the buffer, use this to detect sentinel changes with EOF behaviour (macOS exclusive).
        In doing the `getdirentries64` syscalls, we zero the last four bytes, so they're guaranteed initialised.
        If this marker changes, the kernel has indicated EOF, the buffer is never filled up the the maximum
        check explanation below
        Alignment of 8 => Alignment of 4 guaranteed invariant. */
        // SAFETY: see above
        let last_four_bytes = unsafe {
            core::ptr::NonNull::from_mut(&mut self.syscall_buffer)
                .byte_add(const { Self::BUFFER_SIZE - 4 })
                .cast::<u32>()
        };

        #[cfg(has_eof_trick)]
        /*
        If using the EOF trick, initialise the last 4 bytes of the buffer with 0,
        this means that we detect when the kernel writes it's EOF flags
        Write a 0 to initialise the memory, we check if this changes to 1 after the syscall
        */
        // SAFETY: guaranteed inbounds+valid alignment
        unsafe {
            last_four_bytes.write(0)
        };

        // Get the syscall return amount in bytes
        let remaining_bytes = self.getdents();

        let is_more_remaining = remaining_bytes.is_positive();

        /*
        https://github.com/apple-oss-distributions/Libc/blob/899a3b2d52d95d75e05fb286a5e64975ec3de757/gen/FreeBSD/readdir.c
          Only macOS has this optimisation, the other platforms do not, if macos does not have this, default to checking for 0 ret value
         Check the last four bytes for the marker
        Also the XNU kernel never fills buffer up to maximum size, it always has a flags section towards the end.
        https://github.com/apple/darwin-xnu/blob/main/bsd/sys/dirent.h
        readdr uses the same implementation


            #if __DARWIN_64_BIT_INO_T
            /*
             * sufficiently recent kernels when the buffer is large enough,
             * will use the last bytes of the buffer to return status.
             *
             * To support older kernels:
             * - make sure it's 0 initialized
             * - make sure it's past `dd_size` before reading it
             */
            getdirentries64_flags_t *gdeflags =
                (getdirentries64_flags_t *)(dirp->dd_buf + dirp->dd_len -
                sizeof(getdirentries64_flags_t));
            *gdeflags = 0;
            initial_seek = dirp->dd_td->seekoff;
            dirp->dd_size = (long)__getdirentries64(dirp->dd_fd,
                dirp->dd_buf, dirp->dd_len, &dirp->dd_td->seekoff);
            if (dirp->dd_size >= 0 &&
                dirp->dd_size <= dirp->dd_len - sizeof(getdirentries64_flags_t)) {
                if (*gdeflags & GETDIRENTRIES64_EOF) {
                    dirp->dd_flags |= __DTF_ATEND;
                }
            }

        https://github.com/apple-oss-distributions/Libc/blob/899a3b2d52d95d75e05fb286a5e64975ec3de757/gen/FreeBSD/opendir.c#L373-L392
        As this has existed for decades, it's relatively stable, any ABI breaks are unlikely since we're on 64bit for good.
        */
        #[cfg(has_eof_trick)]
        {
            const { assert!(Self::BUFFER_SIZE >= 1024, "Buffer must be >=1024 for trick") }; // #define GETDIRENTRIES64_EXTENDED_BUFSIZE  1024
            // Here we mirror macos check for flags.
            #[cfg(not(all(target_os = "macos", target_pointer_width = "64")))]
            compile_error!("THIS OPTIMISATION IS ONLY AVAILABLE ON 64-BIT MACOS"); // if accidentally enabled later down the line.
            // check if the syscall did not return the max buffer size, this is extremely unlikely in any case.
            let eof_marker_is_valid: bool = is_more_remaining   // only cast to usize if known to be positive
                && remaining_bytes.cast_unsigned() <= const { Self::BUFFER_SIZE - 4 };

            self.end_of_stream = (eof_marker_is_valid
            /*
            SAFETY:
            - the marker was initialised before the syscall;
            - eof_marker_is_valid proves the returned payload did not  overlap it;
            - the pointer is aligned and in bounds.
            */
            && unsafe { last_four_bytes.read()} & 1 != 0)
                || !is_more_remaining //finally check if the syscall is 0<= for a *paranoid* backup.
        }

        // Check at build time for the optimisation
        // check if the syscall returns >=0 too, the latter branch should almost never be true on supported system
        #[cfg(not(has_eof_trick))]
        {
            self.end_of_stream = !is_more_remaining //gosh this a lot simpler isnt it.
        }

        self.remaining_bytes = remaining_bytes.max(0).cast_unsigned(); // all negatives mapped to 0.

        self.offset = 0; //return the offset back to 0 as now the buffer is cleared

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

        # Safety
        The directory file descriptor is always valid and open for the lifetime of `self` (the destructor closes it).
        The returned pointer is only valid until the next call to `get_next_entry`; it must be
        consumed (e.g. via `construct_entry`) before advancing the stream again.
    */
    #[inline]
    pub unsafe fn get_next_entry(&mut self) -> Option<Unique<dirent64>> {
        while self.offset >= self.remaining_bytes {
            if !self.are_more_entries_remaining() {
                return None;
            }
        }

        // We have data in buffer, get next entry
        // SAFETY: the buffer is not empty, so there are remaining bytes to read,
        // the offset is in bounds, and the buffer is properly aligned.
        let drnt = unsafe {
            Unique::from_ref(&self.syscall_buffer)
                .byte_add(self.offset())
                .cast::<dirent64>()
        };
        debug_assert!(drnt.is_aligned(), "Expected to be aligned, report this");

        // Advance past this entry.
        // SAFETY: we just obtained this pointer, so reading d_reclen is valid.
        let reclen = unsafe { drnt.d_reclen() };
        // We trust reclen is kernel-provided and >= 24 On linux/android
        #[cfg(any(target_os = "linux", target_os = "android"))]
        debug_assert!(reclen >= 24, "expected reclen >=24, it is `{reclen}`");
        self.offset += reclen;

        Some(drnt)
    }

    #[inline]
    pub(crate) fn new(dir: &DirEntry) -> Result<Self> {
        let fd = dir.open()?; //getting the file descriptor

        let path = PathBuffer::new(dir);

        Ok(Self {
            fd,
            syscall_buffer: SyscallBuffer::new(),
            path,
            parent_depth: dir.depth,
            offset: 0,
            remaining_bytes: 0,
            end_of_stream: false,
            #[cfg(any(target_os = "macos", target_os = "freebsd"))]
            base_pointer: 0,
        })
    }
    /**
    Constructs a `GetDents` from a pre-opened file descriptor, skipping the `open()` call.

    Used when the caller already holds an fd obtained via `openat`, avoiding a second
    full-path resolution for the child directory.
    Used internally only due to non-enforceable invariants
    */
    #[inline]
    pub(crate) fn from_fd(fd: OwnedFd, dir: &DirEntry) -> Self {
        let path = PathBuffer::new(dir);

        Self {
            fd,
            syscall_buffer: SyscallBuffer::new(),
            path,
            parent_depth: dir.depth,
            offset: 0,
            remaining_bytes: 0,
            end_of_stream: false,
            #[cfg(any(target_os = "macos", target_os = "freebsd"))]
            base_pointer: 0,
        }
    }
}
// Cheap macro to avoid duplicate code maintenance. (Keep the documentation continuous)
macro_rules! impl_iterator_public_methods {
    ($type:ty) => {
        impl ::std::os::fd::AsRawFd for $type {
            #[inline]
            fn as_raw_fd(&self) -> ::std::os::fd::RawFd {
                self.fd.as_raw_fd()
            }
        }

        impl ::std::os::fd::AsFd for $type {
            #[inline]
            fn as_fd(&self) -> ::std::os::fd::BorrowedFd<'_> {
                self.fd.as_fd()
            }
        }

        impl Iterator for $type {
            type Item = $crate::fs::DirEntry;

            #[inline]
            fn next(&mut self) -> ::core::option::Option<Self::Item> {
                // SAFETY: the underlying fd/directory stream is kept open and valid for the lifetime of `self`
                while let Some(drnt) = unsafe { self.get_next_entry() } {
                    skip_dot_or_dot_dot_entries!(drnt, continue);
                    // avoids using strlen/strcmp on the pointer (avoids unnecessary non inlineable function calls)
                    return Some(self.construct_direntry(drnt));
                }
                None // signal end
            }
        }

        impl $type {
            /**
             Constructs a `DirEntry` from a directory entry pointer.

             This method converts a raw `dirent64` pointer into a safe `DirEntry`
             by combining the directory entry metadata with the parent directory's
             path information stored in the path buffer.

             # Arguments
             * `drnt` - A `Unique` pointer to a valid `dirent64` structure

            */
            #[inline]
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
            fn path_buffer(&mut self) -> &mut PathBuffer {
                &mut self.path
            }

            #[inline]
            fn parent_depth(&self) -> u32 {
                self.parent_depth
            }
        }
    };
}

// Common to all platforms
impl_iterator_public_methods!(ReadDir<'_>);
impl_dirent_constructor!(ReadDir<'_>);

#[cfg(has_faster_readdir)]
impl_iterator_public_methods!(GetDents);
#[cfg(has_faster_readdir)]
impl_dirent_constructor!(GetDents);
