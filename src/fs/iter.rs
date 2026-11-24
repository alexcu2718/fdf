use crate::fs::{DirEntry, FileType, OwnedDir, Result};
#[cfg(has_faster_readdir)]
use crate::fs::{OwnedFdDir, SyscallBuffer};
use crate::{Unique, dirent64};
use core::cell::Cell;
use core::ffi::CStr;
use core::hint::assert_unchecked;
use core::mem::MaybeUninit;
use libc::{AT_SYMLINK_NOFOLLOW, fstatat};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};

// struct DirEntryRef<'fdes>{

//     path:&'fdes CStr,
//     inode:u64,

// }

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
    fn new(path: &DirEntry) -> Self {
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

impl From<&DirEntry> for PathBuffer {
    #[inline]
    fn from(dent: &DirEntry) -> Self {
        Self::new(dent)
    }
}

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
    #[allow(clippy::return_and_then)]
    pub(crate) fn new(dir_path: &DirEntry) -> Result<Self> {
        // Mutate the buffer to contain the full path, then add a null terminator and record the new length
        // We use this length to index to get the filename (store full path -> index to get filename)
        dir_path.open().and_then(|fd| Self::from_fd(fd, dir_path))
    }

    /// Constructs a `ReadDir` from a pre-opened file descriptor, skipping the `open()` call.
    ///
    /// Used when the caller already holds an fd obtained via `openat`, avoiding a second
    /// full-path resolution for the child directory.
    #[inline]
    pub(crate) fn from_fd(dfd: OwnedFd, dir_path: &DirEntry) -> Result<Self> {
        Ok(Self {
            fd: dfd.try_into()?,
            path: dir_path.into(),
            parent_depth: dir_path.depth,
        })
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
        let meta = self.construct_path(drnt);
        DirEntry {
            path: meta.0.into(), //&cstr->cstring
            inode: meta.1,       //u64
            file_type: meta.2,   // filetype
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
            self,
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
        use core::slice::from_raw_parts;
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
        let bytes = unsafe { from_raw_parts(buffer.as_ptr().cast(), required_len) };
        // SAFETY: guraranteed to have no interior nulls, length is suited etc etc.
        let full_path = unsafe { CStr::from_bytes_with_nul_unchecked(bytes) };
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
pub struct GetDents<'filedes> {
    /// The owned, buffered directory file descriptor, closed when the iterator is dropped.
    fd: OwnedFdDir<'filedes>,
    /// Bookmarked base-path state (buffer + filename index) used to construct entry paths
    /// Reused for each entry to avoid repeated memory allocation (only constructed once per dir)
    path: PathBuffer,
    /// Depth of the parent directory in the directory tree hierarchy
    /// Used to calculate depth for child entries during recursive traversal
    parent_depth: u32,
}

#[cfg(has_faster_readdir)]
impl GetDents<'_> {
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
        self.fd.getdents()
    }

    #[inline]
    #[must_use]
    /// Returns a boolean indicating whether the stream is exhausted
    pub const fn is_end_of_stream(&self) -> bool {
        self.fd.is_end_of_stream()
    }

    /// A constant representing the maximum size of the internal Stack based buffer on this platform
    /// Differs per platform and in debug/release! Do not rely on this except if you're doing pointer arithmetic.
    pub const BUFFER_SIZE: usize = SyscallBuffer::BUFFER_SIZE;

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

        # Safety
        The directory file descriptor is always valid and open for the lifetime of `self` (the destructor closes it).
        The returned pointer is only valid until the next call to `get_next_entry`; it must be
        consumed (e.g. via `construct_entry`) before advancing the stream again.
    */
    #[inline]
    pub unsafe fn get_next_entry(&mut self) -> Option<Unique<dirent64>> {
        self.fd.next_pointer()
    }

    #[inline]
    pub(crate) fn new(dir: &DirEntry) -> Result<Self> {
        dir.open().map(|fd| Self::from_fd(fd, dir))
    }
    /**
    Constructs a `GetDents` from a pre-opened file descriptor, skipping the `open()` call.

    Used when the caller already holds an fd obtained via `openat`, avoiding a second
    full-path resolution for the child directory.
    Used internally only due to non-enforceable invariants
    */
    #[inline]
    pub(crate) fn from_fd(fd: OwnedFd, dir: &DirEntry) -> Self {
        Self {
            fd: fd.into(),
            path: dir.into(),
            parent_depth: dir.depth,
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
impl_iterator_public_methods!(GetDents<'_>);
#[cfg(has_faster_readdir)]
impl_dirent_constructor!(GetDents<'_>);
