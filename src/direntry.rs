#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::single_call_fn)]
#![allow(clippy::ptr_as_ptr)] //i know what i'm doing.
#![allow(clippy::integer_division)] //i know my division is safe.
#![allow(clippy::items_after_statements)] //this is just some macro collision,stylistic,my pref.
#![allow(clippy::cast_lossless)]
#[allow(unused_imports)]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use crate::{prefetch_next_buffer, prefetch_next_entry, utils::close_asm, utils::open_asm};

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
use crate::{
    PathBuffer, SyscallBuffer, construct_path, init_path_buffer, offset_ptr,
    skip_dot_or_dot_dot_entries,
};

#[derive(Clone)]
pub struct DirEntry<S>
//S is a storage type, this is used to store the path of the entry, it can be a Box, Arc, Vec, etc.
//ordered by size, so Box<[u8]> is 16 bytes, Arc<[u8]> is 24 bytes, Vec<u8> is 24 bytes, SlimerBox<[u8], u16> is 10 bytes
//S is a generic type that implements BytesStorage trait aka  vec/arc/box/SlimmerBytes(SlimmerBox<[u8], u16>).
//Slimmerbox is Box<[u8]> on non linux/macos due to package limitations,TBD.
where
    S: BytesStorage,
{
    pub(crate) path: OsBytes<S>, //10 bytes,this is basically a box with a much thinner pointer, it's 10 bytes instead of 16.
    pub(crate) file_type: FileType, //1 byte
    pub(crate) inode: u64,       //8 bytes, i may drop this in the future, it's not very useful.
    pub(crate) depth: u8, //1 bytes    , this is a max of 255 directories deep, it's also 1 bytes so keeps struct below 24bytes.
    pub(crate) file_name_index: u16, //2 bytes     , this info is free and helps to get the filename.its formed by path length until  and including last /.
                              //total 22 bytes
                              //2 bytes padding, possible uses? not sure.
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
        matches!(self.file_type, FileType::BlockDevice)
    }

    ///Cost free check for character devices
    #[inline]
    #[must_use]
    pub const fn is_char_device(&self) -> bool {
        matches!(self.file_type, FileType::CharDevice)
    }

    ///Cost free check for fifos
    #[inline]
    #[must_use]
    pub const fn is_fifo(&self) -> bool {
        matches!(self.file_type, FileType::Fifo)
    }

    ///Cost free check for sockets
    #[inline]
    #[must_use]
    pub const fn is_socket(&self) -> bool {
        matches!(self.file_type, FileType::Socket)
    }

    ///Cost free check for regular files
    #[inline]
    #[must_use]
    pub const fn is_regular_file(&self) -> bool {
        matches!(self.file_type, FileType::RegularFile)
    }

    ///Cost free check for directories
    #[inline]
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        matches!(self.file_type, FileType::Directory)
    }
    ///cost free check for unknown file types
    #[inline]
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self.file_type, FileType::Unknown)
    }
    ///cost free check for symlinks
    #[inline]
    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        matches!(self.file_type, FileType::Symlink)
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
    ///Converts a path to a proper path, if it is not already
    pub fn as_full_path(self) -> Result<Self> {
        if self.is_absolute() {
            //doesnt convert
            return Ok(self);
        }
        //safe because easily fits in capacity (which is absurdly big for our purposes)
        let full_path = self.realpath()?;
        let boxed = Self {
            path: full_path.into(),
            file_type: self.file_type,
            inode: self.inode,
            depth: self.depth,
            file_name_index: (full_path.len() - self.file_name().len()) as _,
        }; //we need the length up to the filename INCLUDING
        //including for slash, so eg ../hello/etc.txt has total len 16, then its base_len would be 16-7=9bytes
        //so we subtract the filename length from the total length, probably could've been done more elegantly.
        //TBD? not imperative.

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
        #[cfg(any(
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))]
        let inode = get_stat.st_ino as u64;
        #[cfg(not(any(
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        )))]
        let inode = get_stat.st_ino; //on linux, the inode is a u64 anyway (avoid redundant casts)

        Ok(Self {
            path: path_ref.into(),
            file_type: FileType::from_mode(get_stat.st_mode),
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
        let fd = self
            .as_cstr_ptr(|ptr| unsafe { open(ptr, O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) });
        //let fd = unsafe { open_asm(self) }; //alternative explorative syntax using asm

        if fd < 0 {
            return Err(Error::last_os_error().into());
        }

        let (path_len, path_buffer) = unsafe { init_path_buffer!(self) };
        //using macros because I was learning macros and they help immensely with readability
        //this is a macro that initialises the path buffer, it returns the length of the
        //path and the path buffer itself, which is a stack allocated buffer that can hold the
        //full path of the directory entry, it is used to construct the full path of the
        //directory entry, it is reused for each entry, so we don't have to allocate to the heap until the final point
        //i wish to fix this in future versions

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
    #[cfg(not(target_os = "linux"))]
    #[inline] //back up because we cant use getdents on non linux systems, so we use readdir instead
    #[allow(clippy::missing_errors_doc)]
    pub fn getdents(&self) -> Result<impl Iterator<Item = Self> + '_> {
        DirIter::new(self)
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
    /// Returns the base length of the path buffer.
    #[inline]
    pub const fn file_name_index(&self) -> usize {
        self.file_name_index as _
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
        loop {
            // If we have remaining data in buffer, process it
            if self.offset < self.remaining_bytes as usize {
                let d: *const libc::dirent64 =
                    unsafe { self.buffer.next_getdents_read(self.offset) }; //get next entry in the buffer,
                // this is a pointer to the dirent64 structure, which contains the directory entry information
                #[cfg(target_arch = "x86_64")]
                prefetch_next_entry!(self); /* check how much is left remaining in buffer, if reasonable to hold more, warm cache */
                // Extract the first necessary fieldfrom the dirent structure
                //increment the offset by the size of the dirent structure, this is a pointer to the next entry in the buffer
                self.offset += unsafe { offset_ptr!(d, d_reclen) }; //index to next entry, so when we call next again, we will get the next entry in the buffer

                // skip entries that are not valid or are dot entries

                skip_dot_or_dot_dot_entries!(d, continue); //provide the continue keyword to skip the current iteration if the entry is invalid or a dot entry
                //extract the remaining ones
                let (d_type, inode) = unsafe {
                    (
                        *offset_ptr!(d, d_type), //get the d_type from the dirent structure, this is the type of the entry
                        offset_ptr!(d, d_ino), //get the inode (u32/u64 depending on OS), cast to u64 for consistency
                    )
                };

                let full_path = unsafe { construct_path!(self, d) }; //here we have a construct_path, forms the full path
                //does a lot of black magic, dont worrry about it :)

                let entry = DirEntry {
                    path: full_path.into(),
                    file_type: FileType::from_dtype_fallback(d_type, full_path), //if d_type is unknown fallback to lstat otherwise we get for freeeeeeeee
                    inode,
                    depth: self.parent_depth + 1, // increment depth for child entries
                    file_name_index: self.file_name_index,
                };

                return Some(entry);
            }
            // prefetch the next buffer content before reading
            #[cfg(target_arch = "x86_64")]
            prefetch_next_buffer!(self);
            // check remaining bytes
            self.remaining_bytes = unsafe { self.buffer.getdents64(self.fd) };
            //self.remaining_bytes = unsafe { self.buffer.getdents64_asm(self.fd) }; //see for asm implemetation
            self.offset = 0;
            if self.remaining_bytes <= 0 {
                // If no more entries, return None,
                return None;
            }
        }
    }
}
