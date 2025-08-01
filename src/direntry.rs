#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::single_call_fn)]
#![allow(clippy::ptr_as_ptr)] //i know what i'm doing.
#![allow(clippy::integer_division)] //i know my division is safe.
#![allow(clippy::items_after_statements)] //this is just some macro collision,stylistic,my pref.
#![allow(clippy::cast_lossless)]
#[allow(unused_imports)]
use crate::{AlignedBuffer, LOCAL_PATH_MAX, temp_dirent::TempDirent, utils::resolve_inode};
#[allow(unused_imports)]
#[cfg(target_os = "linux")]
use crate::{close_asm, open_asm};

#[allow(unused_imports)]
use libc::{O_CLOEXEC, O_DIRECTORY, O_NONBLOCK, O_RDONLY, X_OK, access, close, open};
#[allow(unused_imports)]
use std::{ffi::OsStr, io::Error, marker::PhantomData, os::unix::ffi::OsStrExt};

#[allow(unused_imports)]
use crate::{
    BytePath,
    DirIter,
    OsBytes,
    Result,
    cstr,
    custom_types_result::BytesStorage,
    filetype::FileType,
    // utils::unix_time_to_system_time,
};

#[cfg(target_os = "linux")]
use crate::{PathBuffer, SyscallBuffer, offset_dirent};

/// A struct representing a directory entry.
///
/// `S` is a storage type (e.g., `Box<[u8]>`, `Arc<[u8]>`, `Vec<u8>`) used to hold the path bytes.
#[derive(Clone)]
pub struct DirEntry<S>
where
    S: BytesStorage,
{
    /// Path to the entry, stored as OS-native bytes.
    ///
    /// This is a thin pointer wrapper around the storage `S`, optimized for size (~10 bytes). ( on linux/macos, not tested bsds etc)
    pub(crate) path: OsBytes<S>,

    /// File type (file, directory, symlink, etc.).
    ///
    /// Stored as a 1-byte enum.
    pub(crate) file_type: FileType,

    /// Inode number of the file.
    ///
    /// 8 bytes, (may be hidden under a cfg flag in future for relevant reasons/32bit systems(if i ever do that.))
    pub(crate) inode: u64,

    /// Depth of the directory entry relative to the root.
    ///
    /// Stored as a single byte, supporting up to 255 levels deep.
    pub(crate) depth: u8,

    /// Offset in the path buffer where the file name starts.
    ///
    /// This helps quickly extract the file name from the full path.
    pub(crate) file_name_index: u16,
    // 2 bytes free here., we need to leave 1 byte free so we can save on the size of the option/result enum.
    //i'm not sure what's a good use of 1 byte
}

impl<S> DirEntry<S>
where
    S: BytesStorage,
{
    #[inline]
    #[must_use]
    ///costly check for executables
    pub fn is_executable(&self) -> bool {
        //X_OK is the execute permission, requires access call
        self.is_regular_file() && unsafe { self.as_cstr_ptr(|ptr| access(ptr, X_OK) == 0) }
    }

    ///cost free check for block devices
    #[inline]
    #[must_use]
    pub const fn is_block_device(&self) -> bool {
        self.file_type.is_block_device()
    }

    ///Cost free check for character devices
    #[inline]
    #[must_use]
    pub const fn is_char_device(&self) -> bool {
        self.file_type.is_char_device()
    }

    ///Cost free check for pipes (FIFOs)
    #[inline]
    #[must_use]
    pub const fn is_pipe(&self) -> bool {
        self.file_type.is_pipe()
    }

    ///Cost free check for sockets
    #[inline]
    #[must_use]
    pub const fn is_socket(&self) -> bool {
        self.file_type.is_socket()
    }

    ///Cost free check for regular files
    #[inline]
    #[must_use]
    pub const fn is_regular_file(&self) -> bool {
        self.file_type.is_regular_file()
    }

    ///Cost free check for directories
    #[inline]
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        self.file_type.is_dir()
    }
    ///cost free check for unknown file types
    #[inline]
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        self.file_type.is_unknown()
    }
    ///cost free check for symlinks
    #[inline]
    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        self.file_type.is_symlink()
    }
    #[inline]
    #[must_use]
    ///costly check for empty files
    ///i dont see much use for this function
    /// returns false for errors/char devices/sockets/fifos/etc, mostly useful for files and directories
    /// for files, it checks if the size is zero without loading all metadata
    /// for directories, it checks if they have no entries
    /// for special files like devices, sockets, etc., it returns false
    pub fn is_empty(&self) -> bool {
        match self.file_type() {
            FileType::RegularFile => {
                self.size().is_ok_and(|size| size == 0)
                //this checks if the file size is zero, this is a costly check as it requires a stat call
            }
            FileType::Directory => {
                self.readdir() //if we can read the directory, we check if it has no entries
                    .is_ok_and(|mut entries| entries.next().is_none())
            }
            _ => false,
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    ///Converts a dirent64 to a proper path, resolving all symlinks, etc,
    /// there's no way ahead of time to tell if a path has symbolic components.
    /// Returns an error on invalid path (errors to be filled in later)  (they're actually encoded though)
    pub fn to_full_path(self) -> Result<Self> {
        // SAFETY: the filepath must be less than `LOCAL_PATH_MAX` (default, 4096)  (PATH_MAX but can be setup via envvar for testing)
        let ptr = unsafe {
            self.as_cstr_ptr(|cstrpointer| libc::realpath(cstrpointer, std::ptr::null_mut())) //we've created this pointer, we need to be careful
        };

        if ptr.is_null() {
            //check for null
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: pointer is guaranteed null terminated by the kernel, the pointer is properly aligned
        let full_path = unsafe { &*std::ptr::slice_from_raw_parts(ptr.cast(), libc::strlen(ptr)) }; //get length without null terminator
        // we're dereferencing a valid poiinter here, it's fine.
        //alignment is trivial, we use `libc::strlen` because it's probably the most optimal for possibly long paths
        // unfortunately my asm implementation doesn't perform well on long paths, which i want to figure out

        let boxed = Self {
            path: full_path.into(),
            file_type: self.file_type,
            inode: self.inode,
            depth: self.depth,
            file_name_index: full_path.file_name_index() as _,
        }; //we need the length up to the filename INCLUDING
        //including for slash, so eg ../hello/etc.txt has total len 16, then its base_len would be 16-7=9bytes
        //so we subtract the filename length from the total length, probably could've been done more elegantly.
        //TBD? not imperative.
        unsafe { libc::free(ptr as _) }
        //free the pointer to stop leaking (trivial concern considering how little this is going to be called.)

        Ok(boxed)
    }

    #[inline]
    #[allow(clippy::missing_const_for_fn)]
    //this cant be const clippy be LYING AGAIN, this cant be const with slimmer box as it's misaligned,
    //so in my case, because it's 10 bytes, we're looking for an 8 byte reference, so it doesnt work
    #[must_use]
    ///Cost free conversion to bytes (because it is already is bytes)
    pub fn as_bytes(&self) -> &[u8] {
        self
    }

    #[inline]
    #[cfg(target_os = "linux")]
    pub fn to_temp_dirent(&self) -> TempDirent<'_, S> {
        TempDirent {
            path: self.path.as_bytes(),
            inode: self.inode,
            file_type: self.file_type,
            file_name_index: self.file_name_index as _,
            depth: self.depth as _,
            _marker: PhantomData::<S>,
        }
    }

    #[inline]
    #[must_use]
    ///returns the file type of the file (eg directory, regular file, etc)
    pub const fn file_type(&self) -> FileType {
        self.file_type
    }

    #[inline]
    #[must_use]
    ///Returns the depth relative to the start directory, this is cost free
    pub const fn depth(&self) -> usize {
        self.depth as _
    }

    #[inline]
    #[must_use]
    ///Returns the name of the file (as bytes)
    pub fn file_name(&self) -> &[u8] {
        unsafe { self.get_unchecked(self.file_name_index()..) }
    }

    #[inline]
    #[must_use]
    ///returns the inode number of the file, cost free check
    /// this is a unique identifier for the file on the filesystem, it is not the same
    /// as the file name or path, it is a number that identifies the file on the
    /// It should be u32 on BSD's but I use u64 for consistency across platforms
    pub const fn ino(&self) -> u64 {
        self.inode
    }

    #[inline]
    #[must_use]
    ///an internal function apply a function, this will be public probably when I figure out api
    pub fn filter<F: Fn(&Self) -> bool>(&self, func: F) -> bool {
        func(self)
    }

    #[inline]
    #[must_use]
    ///returns the length of the base path (eg /home/user/ is 6 '/home/')
    pub const fn file_name_index(&self) -> usize {
        self.file_name_index as _
    }

    #[inline]
    #[must_use]
    ///Checks if the file is a directory or symlink, this is a cost free check
    pub const fn is_traversible(&self) -> bool {
        //this is a cost free check, we just check if the file type is a directory or symlink
        matches!(self.file_type, FileType::Directory | FileType::Symlink)
    }

    #[inline]
    #[must_use]
    ///checks if the file is hidden eg .gitignore
    pub fn is_hidden(&self) -> bool {
        unsafe { *self.get_unchecked(self.file_name_index()) == b'.' } //we yse the base_len as a way to index to filename immediately, this means
        //we can store a full path and still get the filename without copying.
        //this is safe because we know that the base_len is always less than the length of the path
    }
    #[inline]
    #[must_use]
    ///returns the directory name of the file (as bytes) or failing that (/ is problematic) will return the full path,
    pub fn dirname(&self) -> &[u8] {
        unsafe {
            self //this is why we store the baseline, to check this and is hidden as babove, its very useful and cheap
                .get_unchecked(..self.file_name_index() - 1)
                .rsplit(|&b| b == b'/')
                .next()
                .unwrap_or(self.as_bytes())
        }
    }

    #[inline]
    #[must_use]
    ///returns the parent directory of the file (as bytes)
    pub fn parent(&self) -> &[u8] {
        unsafe { self.get_unchecked(..std::cmp::max(self.file_name_index() - 1, 1)) }

        //we need to be careful if it's root,im not a fan of this method but eh.
        //theres probably a more elegant way.
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    ///Creates a new `DirEntry` from a path
    /// Rreturns a `Result<DirEntry, DirEntryError>`.
    /// This will error if path isn't valid/permission problems etc.
    pub fn new<T: AsRef<OsStr>>(path: T) -> Result<Self> {
        let path_ref = path.as_ref().as_bytes();

        // extract information from successful stat
        let get_stat = path_ref.get_stat()?;
        let inode = resolve_inode(&get_stat); //resolves inode to u64 but avoids redundant cast
        Ok(Self {
            path: path_ref.into(),
            file_type: get_stat.into(),
            inode,
            depth: 0,
            file_name_index: path_ref.file_name_index(),
        })
    }

    /// Returns an iterator over the directory entries using `readdir64` as opposed to `getdents`, this uses a higher level api
    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn readdir(&self) -> Result<impl Iterator<Item = Self> + '_> {
        DirIter::new(self)
    }
    #[inline]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    #[cfg(target_os = "linux")]
    ///`getdents` is an iterator over fd,where each consequent index is a directory entry.
    /// This function is a low-level syscall wrapper that reads directory entries.
    /// It returns an iterator that yields `DirEntry` objects.
    /// This differs from my `as_iter` impl, which uses libc's `readdir64`, this uses `libc::syscall(SYS_getdents64.....)`
    /// which in theory allows it to be offered tuned parameters, such as a high buffer size (shows performance benefits)
    ///  you can likely make the stack copies extremely cheap
    /// EG I use a ~4.1k buffer, which is about close to the max size for most dirents, meaning few will require more than one.
    /// but in actuality, i should/might parameterise this to allow that, i mean its trivial, its about 10 lines in total.
    pub fn getdents(&self) -> Result<impl Iterator<Item = Self> + '_> {
        //matching type signature of above for consistency
        let fd = unsafe { self.open_fd()? }; //returns none if null (END OF DIRECTORY/Directory no longer exists) (we've already checked if it's a directory/symlink originally )
        let mut path_buffer = crate::AlignedBuffer::<u8, { crate::LOCAL_PATH_MAX }>::new(); //nulll initialised  (stack) buffer that can axiomatically hold any filepath.

        let path_len = unsafe { path_buffer.init_from_direntry(self) };
        //TODO! make this more ergonomic

        Ok(DirEntryIterator {
            fd,
            buffer: SyscallBuffer::new(),
            path_buffer,
            file_name_index: path_len as _,
            parent_depth: self.depth,
            offset: 0,
            remaining_bytes: 0,
            _marker: PhantomData::<S>, // marker for the storage type, this is used to ensure that the iterator can be used with any storage type
        })
    }
}

#[cfg(target_os = "linux")]
///Iterator for directory entries using getdents syscall
pub struct DirEntryIterator<S>
where
    S: BytesStorage,
{
    pub(crate) fd: i32, //fd, this is the file descriptor of the directory we are reading from(it's completely useless after the iterator is dropped)
    pub(crate) buffer: SyscallBuffer, // buffer for the directory entries, this is used to read the directory entries from the  syscall IO, it is 4.1k bytes~ish in size
    pub(crate) path_buffer: PathBuffer, // buffer(stack allocated) for the path, this is used to construct the full path of the entry, this is reused for each entry
    pub(crate) file_name_index: u16, // base path length, this is the length of the path up to and including the last slash (we use these to get filename trivially)
    pub(crate) parent_depth: u8, // depth of the parent directory, this is used to calculate the depth of the child entries
    pub(crate) offset: usize, // offset in the buffer, this is used to keep track of where we are in the buffer
    pub(crate) remaining_bytes: i64, // remaining bytes in the buffer, this is used to keep track of how many bytes are left to read
    _marker: PhantomData<S>, // marker for the storage type, this is used to ensure that the iterator can be used with any storage type
                             //this gets compiled away anyway as its as a zst
}
#[cfg(target_os = "linux")]
impl<S> Drop for DirEntryIterator<S>
where
    S: BytesStorage,
{
    /// Drops the iterator, closing the file descriptor.
    /// we need to close the file descriptor when the iterator is dropped to avoid resource leaks.
    /// basically you can only have X number of file descriptors open at once, so we need to close them when we are done.
    #[inline]
    fn drop(&mut self) {
        unsafe { close(self.fd) };
        //unsafe { close_asm(self.fd) }; //asm implementation, for when i feel like testing if it does anything useful.
    }
}
#[cfg(target_os = "linux")]
impl<S> DirEntryIterator<S>
where
    S: BytesStorage,
{
    #[inline]
    ///Returns a pointer to the `libc::dirent64` in the buffer then increments the offset by the size of the dirent structure.
    /// this is so that when we next time we call `next_getdents_pointer`, we get the next entry in the buffer.
    /// This is unsafe because it dereferences a raw pointer, so we need to ensure that
    /// the pointer is valid and that we don't read past the end of the buffer.
    /// This is only used in the iterator implementation, so we can safely assume that the pointer
    /// is valid and that we don't read past the end of the buffer.
    pub const unsafe fn next_getdents_pointer(&mut self) -> *const libc::dirent64 {
        let d: *const libc::dirent64 = unsafe { self.buffer.as_ptr().add(self.offset).cast::<_>() };
        self.offset += unsafe { offset_dirent!(d, d_reclen) }; //increment the offset by the size of the dirent structure, this is a pointer to the next entry in the buffer
        d //this is a pointer to the dirent64 structure, which contains the directory entry information
    }
    #[inline]
    /// Checks the remaining bytes in the buffer, and resets the offset to 0.
    /// This is a syscall that returns the number of bytes left to read.
    ///
    ///
    /// This is unsafe because it dereferences a raw pointer, so we need to ensure that
    /// the pointer is valid and that we don't read past the end of the buffer.
    pub unsafe fn getdents_syscall(&mut self) {
        self.remaining_bytes = unsafe { self.buffer.getdents64_internal(self.fd) };
        self.offset = 0;
    }

    #[inline]
    /// Prefetches the next likely entry in the buffer to keep the cache warm.
    pub(crate) fn prefetch_next_entry(&self) {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if self.offset + 128 < self.remaining_bytes as usize {
                unsafe {
                    use std::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
                    let next_entry = self.buffer.as_ptr().add(self.offset + 64).cast();
                    _mm_prefetch(next_entry, _MM_HINT_T0);
                }
            }
        }
    }
    #[inline]
    /// Checks if the buffer is empty
    pub const fn is_buffer_empty(&self) -> bool {
        self.offset < self.remaining_bytes as _
    }

    #[inline]
    /// Prefetches the start of the buffer to keep the cache warm.
    pub(crate) fn prefetch_next_buffer(&self) {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            unsafe {
                use std::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
                _mm_prefetch(self.buffer.as_ptr().cast(), _MM_HINT_T0);
            }
        }
    }

    #[inline]
    /// Checks if we're at end of directory
    pub const fn is_end_of_directory(&self) -> bool {
        self.remaining_bytes <= 0
    }
}

#[cfg(target_os = "linux")]
impl<S> Iterator for DirEntryIterator<S>
where
    S: BytesStorage,
{
    type Item = DirEntry<S>;
    #[inline]
    /// Returns the next directory entry in the iterator.
    fn next(&mut self) -> Option<Self::Item> {
        use crate::traits_and_conversions::DirentConstructor;
        loop {
            // If we have remaining data in buffer, process it
            if self.is_buffer_empty() {
                let d: *const libc::dirent64 = unsafe { self.next_getdents_pointer() }; //get next entry in the buffer,
                // this is a pointer to the dirent64 structure, which contains the directory entry information
                self.prefetch_next_entry(); /* check how much is left remaining in buffer, if reasonable to hold more, warm cache */

                skip_dot_or_dot_dot_entries!(d, continue); //provide the continue keyword to skip the current iteration if the entry is invalid or a dot entry
                //extract non . and .. files
                let entry = unsafe { self.construct_entry(d) }; //construct the dirent from the pointer, this is a safe function that constructs the DirEntry from the dirent64 structure

                return Some(entry);
            }
            // prefetch the next buffer content before reading, only applies if no

            self.prefetch_next_buffer(); //prefetch the next buffer content to keep the cache warm, this is a no-op on non-linux systems
            // issue a syscall once out of entries
            unsafe { self.getdents_syscall() }; //get the remaining bytes in the buffer, this is a syscall that returns the number of bytes left to read

            if self.is_end_of_directory() {
                // If no more entries, return None,
                return None;
            }
        }
    }
}
