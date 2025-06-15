#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::single_call_fn)]
#![allow(clippy::integer_division_remainder_used)]
#![allow(clippy::ptr_as_ptr)] //i know what i'm doing.
#![allow(clippy::integer_division)] //i know my division is safe.
#![allow(clippy::items_after_statements)] //this is just some macro collision,stylistic,my pref.
#![allow(clippy::little_endian_bytes)] //i dont even know why this is a lint(because i cant be bothered to read it, i NEED IT though)
#[allow(unused_imports)]
use libc::{O_CLOEXEC, O_DIRECTORY, O_NONBLOCK, O_RDONLY, X_OK, access, close, dirent64, open};
#[allow(unused_imports)]
use std::{
    convert::TryFrom,
    ffi::OsStr,
    fmt,
    io::Error,
    marker::PhantomData,
    mem::offset_of,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

#[allow(unused_imports)]
use crate::{
    AsU8 as _, BytePath, DirIter, OsBytes, PathBuffer, Result, SyscallBuffer, construct_path, cstr,
    cstr_n, custom_types_result::BytesStorage, filetype::FileType, get_dirent_vals,
    init_path_buffer_syscall, offset_ptr, prefetch_next_buffer, prefetch_next_entry,
    skip_dot_entries, utils::close_asm, utils::open_asm, utils::unix_time_to_system_time,
};

#[derive(Clone)]
pub struct DirEntry<S>
//S is a storage type, this is used to store the path of the entry, it can be a Box, Arc, Vec, etc.
//ordered by size, so Box<[u8]> is 16 bytes, Arc<[u8]> is 24 bytes, Vec<u8> is 24 bytes, SlimerBox<[u8], u16> is 10 bytes
//S is a generic type that implements BytesStorage trait aka  vec/arc/box/SlimmerBytes(SlimmerBox<[u8], u16>).
where
    S: BytesStorage,
{
    pub(crate) path: OsBytes<S>, //10 bytes,this is basically a box with a much thinner pointer, it's 10 bytes instead of 16.
    pub(crate) file_type: FileType, //1 byte
    pub(crate) inode: u64,       //8 bytes, i may drop this in the future, it's not very useful.
    pub(crate) depth: u8, //1 bytes    , this is a max of 255 directories deep, it's also 1 bytes so keeps struct below 24bytes.
    pub(crate) base_len: u16, //2 bytes     , this info is free and helps to get the filename.its formed by path length until  and including last /.
                              //total 22 bytes
                              //2 bytes padding, possible uses? not sure.
                              //due to my pointer checks i could get this for free (bool) but dont really want massive structs
}

impl<S> DirEntry<S>
where
    S: BytesStorage,
{
    #[inline]
    #[must_use]
    ///costly check for executables
    pub fn is_executable(&self) -> bool {
        if !self.is_regular_file() {
            return false;
        }

        unsafe {
            // x_ok checks for execute permission
            self.as_cstr_ptr(|ptr| access(ptr, X_OK) == 0)
        }
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
        if self.is_regular_file() {
            // for files, check if size is zero without loading all metadata
            unsafe { self.size().is_ok_and(|size| size == 0) } //safe because we know it wont overflow.
        } else if self.is_dir() {
            // for directories, check if they have no entries
            self.readdir() //we use readdir here because we want to check `quickly`
                //, getdents is more efficient but for listing directories, finding the first entry is a different case.
                .is_ok_and(|mut entries| entries.next().is_none())
        } else {
            // special files like devices, sockets, etc.
            false
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
            base_len: (full_path.len() - self.file_name().len()) as _,
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
        unsafe { self.get_unchecked(self.base_len()..) }
    }

    #[inline]
    #[must_use]
    ///returns the inode number of the file, rather expensive
    /// i just included it for sake of completeness.
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
    pub const fn base_len(&self) -> usize {
        self.base_len as _
    }

    #[inline]
    #[must_use]
    ///checks if the file is hidden eg .gitignore
    pub fn is_hidden(&self) -> bool {
        unsafe { *self.get_unchecked(self.base_len()) == b'.' } //we yse the base_len as a way to index to filename immediately, this means
        //we can store a full path and still get the filename without copying.
        //this is safe because we know that the base_len is always less than the length of the path
    }
    #[inline]
    #[must_use]
    ///returns the directory name of the file (as bytes) or failing that (/ is problematic) will return the full path,
    pub fn dirname(&self) -> &[u8] {
        unsafe {
            self //this is why we store the baseline, to check this and is hidden as babove, its very useful and cheap
                .get_unchecked(..self.base_len() - 1)
                .rsplit(|&b| b == b'/')
                .next()
                .unwrap_or(self.as_bytes())
        }
    }

    #[inline]
    #[must_use]
    ///returns the parent directory of the file (as bytes)
    pub fn parent(&self) -> &[u8] {
        unsafe { self.get_unchecked(..std::cmp::max(self.base_len as usize - 1, 1)) }

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

        Ok(Self {
            path: path_ref.into(),
            file_type: FileType::from_mode(get_stat.st_mode),
            inode: get_stat.st_ino,
            depth: 0,
            base_len: path_ref.get_baselen(),
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
    #[allow(clippy::cast_possible_wrap)]
    ///`getdents` is an iterator over fd,where each consequent index is a directory entry.
    /// This function is a low-level syscall wrapper that reads directory entries.
    /// It returns an iterator that yields `DirEntry` objects.
    /// This differs from my `as_iter` impl, which uses libc's `readdir64`, this uses `libc::syscall(SYS_getdents64.....)`
    /// which in theory allows it to be offered turned parameters, ie by purposely restriction the depth,
    ///  you can likely make the stack copies extremely cheap
    /// EG I use a ~4.1k buffer, which is about close to the max size for most dirents, meaning few will require more than one.
    /// but in actuality, i should/might parameterise this to allow that, i mean its trivial, its about 10 lines in total.
    pub fn getdents(&self) -> Result<impl Iterator<Item = Self>> {
        let dir_path = self.as_bytes();
        //  let fd = dir_path
        // .as_cstr_ptr(|ptr| unsafe { open(ptr, O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) });
        let fd = unsafe { open_asm(dir_path) };
        //alternatively syntaxes I made.
        //let fd= unsafe{ open(cstr_n!(dir_path,256),O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) };
        //let fd= unsafe{ open(cstr!(dir_path),O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) };

        if fd < 0 {
            return Err(Error::last_os_error().into());
        }

        let mut path_buffer = PathBuffer::new(); // buffer for the path, this is used(the pointer is mutated) to construct the full path of the entry, this is actually
        //a uninitialised buffer, which is then initialised with the directory path
        let mut path_len = dir_path.len();
        init_path_buffer_syscall!(path_buffer, path_len, dir_path, self); // initialise the path buffer with the directory path
        //using macros is ideal here and i need generics

        Ok(DirEntryIterator {
            fd,
            buffer: SyscallBuffer::new(),
            path_buffer,
            base_path_len: path_len as _,
            parent_depth: self.depth,
            offset: 0,
            remaining_bytes: 0,
            _marker: PhantomData::<S>, // marker for the storage type, this is used to ensure that the iterator can be used with any storage type
        })
    }
}

///Iterator for directory entries using getdents syscall
pub struct DirEntryIterator<S>
where
    S: BytesStorage,
{
    pub(crate) fd: i32, //fd, this is the file descriptor of the directory we are reading from, it is used to read the directory entries via syscall
    pub(crate) buffer: SyscallBuffer, // buffer for the directory entries, this is used to read the directory entries from the file descriptor via syscall, it is 4.1k bytes~ish
    pub(crate) path_buffer: PathBuffer, // buffer for the path, this is used to construct the full path of the entry, this is reused for each entry
    pub(crate) base_path_len: u16, // base path length, this is the length of the path up to and including the last slash
    pub(crate) parent_depth: u8, // depth of the parent directory, this is used to calculate the depth of the child entries
    pub(crate) offset: usize, // offset in the buffer, this is used to keep track of where we are in the buffer
    pub(crate) remaining_bytes: i64, // remaining bytes in the buffer, this is used to keep track of how many bytes are left to read
    _marker: PhantomData<S>, // marker for the storage type, this is used to ensure that the iterator can be used with any storage type
                             //this gets compiled away anyway as its as a zst
}

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
                let d: *const dirent64 = unsafe { self.buffer.next_getdents_read(self.offset) }; //get next entry in the buffer,
                // this is a pointer to the dirent64 structure, which contains the directory entry information
                #[cfg(target_arch = "x86_64")]
                prefetch_next_entry!(self);
                // Extract the fields from the dirent structure
                let (name_ptr, d_type, inode, reclen): (*const u8, u8, u64, usize) = //we have to tell our macro what types 
                    get_dirent_vals!(d);

                self.offset += reclen; //index to next entry, so when we call next again, we will get the next entry in the buffer

                // skip entries that are not valid or are dot entries
                //a macro that extracts the values from the dirent structure, this is a niche optimisation,
                skip_dot_entries!(d_type, name_ptr, reclen); //requiring d_type is just a niche optimisation, it allows us not to do 'as many' pointer checks
                //optionally here we can include the reclen, as reclen==24 is when specifically . and .. appear
                //
                // let full_path = unsafe { construct_path!(self, name_ptr) }; //a macro that constructs it, the full details are a bit lengthy
                let full_path = unsafe { crate::construct_path_optimised!(self, d) }; //here we have a construct_path_optimised  version, which uses a very specific trick, i need to benchmark it!

                let entry = DirEntry {
                    path: full_path.into(),
                    file_type: FileType::from_dtype_fallback(d_type, full_path), //if d_type is unknown fallback to lstat otherwise we get for freeeeeeeee
                    inode,
                    depth: self.parent_depth + 1, // increment depth for child entries
                    base_len: self.base_path_len,
                };

                unsafe {
                    debug_assert!(entry.file_name().len() == crate::dirent_const_time_strlen!(d))
                }
                unsafe {
                    debug_assert!(entry.file_name().len() == crate::dirent_const_time_strlen(d))
                }

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
