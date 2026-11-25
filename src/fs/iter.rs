use crate::{
    Unique, dirent64,
    fs::{DirEntry, DirStream, FileType, OwnedDir, Result},
};
use core::{cell::Cell, ffi::CStr, hint::assert_unchecked, mem::MaybeUninit, slice};
use libc::{AT_SYMLINK_NOFOLLOW, fstatat};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};

/**
 Bookmarked path-construction state shared by every directory iterator.

 Combines the base-path buffer with the index marking where filenames start,
 so the two always stay in sync.
*/
#[derive(Debug)]
struct Metadata {
    buffer: Vec<MaybeUninit<u8>>,
    /// Index into `buffer` where filenames start (avoids recalculating)
    file_index: usize,
    /// Depth of the bookmarked base path relative to the traversal root
    depth: u32,
}

impl From<&DirEntry> for Metadata {
    #[inline]
    fn from(dentry: &DirEntry) -> Self {
        Self::new(dentry)
    }
}

impl Metadata {
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

        Self {
            buffer,
            file_index: len + needs_slash,
            depth: path.depth,
        }
        /* for 'len' we only read up to len bytes, so inc the ase it's not a directory, we intentionally omit the last slash by truncation.*/
    }

    #[inline]
    /// Total allocated size of the internal vector that holds the path buffer
    const fn allocated_size(&self) -> usize {
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

    #[inline]
    #[expect(clippy::indexing_slicing, reason = "panic free")]
    /**
    Copies a NUL-terminated directory-entry name (`d_name_cstr`, including its NUL byte) into the
    buffer right after the bookmarked base path, growing the buffer first if required.

    Returns the resulting full path (base path + '/' + name) as a `&CStr`.
    */
    fn copy_dname(&mut self, drnt: &Unique<dirent64>) -> &CStr {
        // SAFETY: just obtained, pointer is aligned, alive etc.
        let d_name_cstr = unsafe { drnt.d_name_cstr() };
        let base_len = self.file_index;
        let name_len = d_name_cstr.count_bytes() + 1; //technically should be a u16 but we need it for indexing :( // we need +1 bc this strlen omits null terminator
        let required_len = base_len + name_len;
        //SAFETY: trivial, name_len=>1 (POSIX standard) ; ALLOWS US TO ELIDE BOUNDS CHECKS
        // rustc knows this but it's unsigned overflow conditions being annoying.
        unsafe { assert_unchecked(required_len > base_len) };

        // Get the amount of the buffer we wish to initialise (grows the buffer on the cold path if required).
        let buffer = self.prepare(required_len);

        // Get the portion of the buffer that goes past the last slash, up to the name+NUL
        let name_portion = &mut buffer[base_len..];

        // Copy name into the uninit memory.
        name_portion.write_copy_of_slice(d_name_cstr.to_bytes_with_nul());
        //^this is a memcpy.
        // SAFETY: the buffer is guaranteed null terminated(copying a null terminated string)
        // &CStr is a &[u8]/&[i8] underneath.
        let bytes: &[u8] = unsafe { slice::from_raw_parts(buffer.as_ptr().cast(), required_len) };
        // SAFETY: guraranteed to have no interior nulls, length is suited etc etc.
        unsafe { CStr::from_bytes_with_nul_unchecked(bytes) }
    }
}

/**
 POSIX-compliant directory iterator using libc's readdir

 This iterator traverses directory entries using the standard POSIX directory
 reading API. It automatically skips "." and ".." entries and provides
 a safe Rust interface over the underlying C library functions.

*/
pub type ReadDir<'filedes> = DirIter<OwnedDir<'filedes>>;

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
pub type GetDents<'filedes> = DirIter<crate::fs::OwnedFdDir<'filedes>>;

/// Directory iterator shared by [`ReadDir`] and [`GetDents`]: a raw entry stream plus the
/// bookmarked base path used to build each entry's full path.
#[derive(Debug)]
pub struct DirIter<S: DirStream> {
    /// The owned directory stream, closed when the iterator is dropped.
    fd: S,
    /// Bookmarked base-path state (buffer + filename index + depth), reused for every entry
    /// (only constructed once per dir).
    path: Metadata,
}

impl<S: DirStream> AsRawFd for DirIter<S> {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl<S: DirStream> AsFd for DirIter<S> {
    #[inline]
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl<S: DirStream> Iterator for DirIter<S> {
    type Item = DirEntry;

    #[inline]
    fn next(&mut self) -> Option<DirEntry> {
        // SAFETY: the underlying fd/directory stream is kept open and valid for the lifetime of `self`
        while let Some(drnt) = unsafe { self.get_next_entry() } {
            // avoids using strlen/strcmp on the pointer (avoids unnecessary non inlineable function calls)
            skip_dot_or_dot_dot_entries!(drnt, continue);
            return Some(self.construct_direntry(&drnt));
        }
        None
    }
}

impl<S: DirStream> DirIter<S> {
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

    /// Opens `dir` and wraps it in an iterator.
    #[inline]
    pub(crate) fn new(dir: &DirEntry) -> Result<Self> {
        Self::from_fd(dir.open()?, dir)
    }

    /// Constructs a `DirIter` from a pre-opened file descriptor, skipping the `open()` call.
    ///
    /// Used when the caller already holds an fd obtained via `openat`, avoiding a second
    /// full-path resolution for the child directory.
    #[inline]
    pub(crate) fn from_fd(dfd: OwnedFd, dir: &DirEntry) -> Result<Self> {
        S::from_owned_fd(dfd)
            .map(|fd| Self {
                fd,
                path: dir.into(),
            })
            .map_err(Into::into)
    }

    /// Constructs a `DirEntry` from a directory entry pointer, combining the entry's metadata
    /// with the parent directory's path stored in the path buffer.
    ///
    /// `drnt` must be a valid pointer freshly obtained from [`Self::get_next_entry`].
    #[inline]
    pub fn construct_direntry(&mut self, drnt: &Unique<dirent64>) -> DirEntry {
        // SAFETY: we just obtained this pointer and it hasn't been invalidated by advancing the stream
        let inode = unsafe { drnt.d_ino() }; //Returns 0 if d_ino isn't defined on your system
        debug_assert!(drnt.is_aligned(), "drnt not aligned(report this)");

        // Only fall back to an `fstatat` when the filesystem doesn't supply `d_type`
        // (skipping the syscall is a massive perf win).
        #[cfg(has_d_type)]
        // SAFETY: as above
        let file_type = match FileType::from_dtype(unsafe { drnt.d_type() }) {
            FileType::Unknown => self.stat_filetype(drnt),
            known => known,
        };
        #[cfg(not(has_d_type))]
        let file_type = self.stat_filetype(&drnt);

        DirEntry {
            path: self.path.copy_dname(drnt).into(),
            inode,
            file_type,
            depth: self.path.depth + 1,
            file_name_index: self.path.file_index,
            is_cached_traversable: Cell::new(None), // Lazy cache for traversal checks
        }
    }

    /// Gets the `FileType` without following symlinks (same semantics as readdir/getdents)
    #[inline]
    fn stat_filetype(&self, drnt: &Unique<dirent64>) -> FileType {
        stat_syscall!(fstatat, self, drnt.d_name(), AT_SYMLINK_NOFOLLOW, DTYPE)
    }
}
