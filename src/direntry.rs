#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::single_call_fn)]

use libc::{
    F_OK, O_CLOEXEC, O_DIRECTORY, O_NONBLOCK, O_RDONLY, R_OK, W_OK, X_OK, access, close, dirent64,
    open, strlen,
};
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
    AsU8 as _, DirIter, OsBytes as _, PathBuffer, Result, SyscallBuffer, ToStat as _,
    construct_path, cstr, cstr_n, custom_types_result::SlimOsBytes, error::DirEntryError,
    filetype::FileType, init_path_buffer_syscall, offset_ptr, prefetch_next_buffer,
    prefetch_next_entry, skip_dot_entries, traits_and_conversions::AsOsStr as _,
    traits_and_conversions::BytesToCstrPointer, utils::get_baselen, utils::open_asm,utils::close_asm,
    utils::unix_time_to_system_time,
};

#[derive(Clone)]
pub struct DirEntry {
    pub(crate) path: SlimOsBytes, //10 bytes,this is basically a box with a much thinner pointer, it's 10 bytes instead of 16.
    pub(crate) file_type: FileType, //1 byte
    pub(crate) inode: u64,        //8 bytes, i may drop this in the future, it's not very useful.
    pub(crate) depth: u8, //1 bytes    , this is a max of 255 directories deep, it's also 1 bytes so keeps struct below 24bytes.
    pub(crate) base_len: u16, //2 bytes     , this info is free and helps to get the filename.its formed by path length until  and including last /.
                              //total 22 bytes
                              //2 bytes padding, possible uses? not sure.
                              //due to my pointer checks i could get this for free (bool) but dont really want massive structs
}

impl fmt::Display for DirEntry {
    //i might need to change this to show other metadata.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str_lossy())
    }
}

impl std::ops::Deref for DirEntry {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl From<DirEntry> for PathBuf {
    fn from(entry: DirEntry) -> Self {
        entry.into_path()
    }
}
impl TryFrom<&[u8]> for DirEntry {
    type Error = DirEntryError;

    fn try_from(path: &[u8]) -> Result<Self> {
        Self::new(OsStr::from_bytes(path))
    }
}

impl TryFrom<&OsStr> for DirEntry {
    type Error = DirEntryError;

    fn try_from(path: &OsStr) -> Result<Self> {
        Self::new(path)
    }
}

impl AsRef<Path> for DirEntry {
    #[inline]
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl fmt::Debug for DirEntry {
    ///debug format for `DirEntry` (showing a vector of bytes is... not very useful)
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DirEntry(Filename:{},FileType:{},Inode:{},Depth:{})",
            self.as_str_lossy(),
            self.file_type,
            self.inode,
            self.depth
        )
    }
}

impl DirEntry {
    #[inline]
    #[must_use]
    ///costly check for executables
    pub fn is_executable(&self) -> bool {
        if !self.is_regular_file() {
            return false;
        }

        unsafe {
            // x_ok checks for execute permission
            self.as_bytes().as_cstr_ptr(|ptr| access(ptr, X_OK) == 0)
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
            self.size().is_ok_and(|size| size == 0)
        } else if self.is_dir() {
            // for directories, check if they have no entries
            self.readdir()
                .is_ok_and(|mut entries| entries.next().is_none())
        } else {
            // special files like devices, sockets, etc.
            false
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    ///resolves the path to an absolute path
    /// this is a costly operation, as it requires a syscall to resolve the path.
    /// unless the path is already absolute, in which case its a trivial operation
    pub fn realpath(&self) -> Result<&[u8]> {
        if self.is_absolute() {
            return Ok(self.as_bytes());
        }
        //cast byte slice into a *const c_char/i8 pointer with a null terminator THEN pass it to realpath along with a null mut pointer
        let ptr = unsafe {
            self.as_bytes()
                .as_cstr_ptr(|cstrpointer| libc::realpath(cstrpointer, std::ptr::null_mut()))
        };
        if ptr.is_null() {
            //check for null
            return Err(Error::last_os_error().into());
        }
        //better to use strlen here because path is likely to be too long to benefit from repne scasb
        //we also use `std::ptr::slice_from_raw_parts`` to  avoid a UB check (trivial but we're leaving safety to user :)))))))))))
        Ok(unsafe { &*std::ptr::slice_from_raw_parts(ptr.cast(), strlen(ptr) as usize) })
    }

    #[inline]
    #[allow(clippy::missing_safety_doc)]
    ///resolves the path to an absolute path
    /// this is a costly operation, as it requires a syscall to resolve the path.
    /// unless the path is already absolute, in which case its a trivial operation
    /// do not use this unless you are sure the path is valid, as it is unsafe.
    pub unsafe fn realpath_unchecked(&self) -> &[u8] {
        if self.is_absolute() {
            return self.as_bytes();
        }

        let ptr = unsafe {
            self.as_bytes()
                .as_cstr_ptr(|cstrpointer| libc::realpath(cstrpointer, std::ptr::null_mut()))
        };
        //we use strlen here because path is likely to be too long to benefit from repne scasb
        unsafe { &*std::ptr::slice_from_raw_parts(ptr.cast::<u8>(), strlen(ptr)) }
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
            base_len: (full_path.len() - self.file_name().len()) as u16,
        }; //we need the length up to the filename INCLUDING
        //including for slash, so eg ../hello/etc.txt has total len 16, then its base_len would be 16-7=9bytes
        //so we subtract the filename length from the total length, probably could've been done more elegantly.
        //TBD? not imperative.

        Ok(boxed)
    }

    #[inline]
    #[must_use]
    ///determines if the path is relative
    pub fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    #[inline]
    #[must_use]
    ///somewhatcostly check for readable files
    pub fn is_readable(&self) -> bool {
        unsafe { self.as_bytes().as_cstr_ptr(|ptr| access(ptr, R_OK)) == 0 }
    }

    #[inline]
    #[must_use]
    ///somewhat costly check for writable files(by current user)
    pub fn is_writable(&self) -> bool {
        //maybe i can automatically exclude certain files from this check to
        //then reduce my syscall total, would need to read into some documentation. zadrot ebaniy
        unsafe { self.as_bytes().as_cstr_ptr(|ptr| access(ptr, W_OK)) == 0 }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    ///returns the std definition of metadata for easy validation/whatever purposes.
    pub fn metadata(&self) -> Result<std::fs::Metadata> {
        std::fs::metadata(self.as_os_str()).map_err(|_| DirEntryError::MetadataError)
    }
    #[inline]
    #[allow(clippy::missing_const_for_fn)]
    //this cant be const clippy be LYING AGAIN, this cant be const with slimmer box as it's misaligned,
    //so in my case, because it's 10 bytes, we're looking for an 8 byte reference, so it doesnt work
    #[must_use]
    ///Cost free conversion to bytes (because it is already is bytes)
    pub fn as_bytes(&self) -> &[u8] {
        self.path.as_bytes()
    }

    #[inline]
    #[must_use]
    ///checks if the path is absolute,
    pub fn is_absolute(&self) -> bool {
        self.as_bytes()[0] == b'/'
    }

    #[inline]
    // Returns an iterator over the components of the path.
    /// This splits the path by '/' and filters out empty components.
    pub fn components(&self) -> impl Iterator<Item = &[u8]> {
        self.as_bytes()
            .split(|&b| b == b'/')
            .filter(|s| !s.is_empty())
    }

    #[inline]
    #[must_use]
    ///Low cost  conversion to a `Path`
    pub fn as_path(&self) -> &Path {
        Path::new(self.as_os_str())
    }

    #[inline]
    #[must_use]
    ///returns the file type of the file (eg directory, regular file, etc)
    pub const fn file_type(&self) -> FileType {
        self.file_type
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    ///Costly conversion to a `std::fs::FileType`
    pub fn to_std_file_type(&self) -> Result<std::fs::FileType> {
        //  can't directly create a std::fs::FileType,
        // we need to make a system call to get it
        std::fs::symlink_metadata(self.as_path())
            .map(|m| m.file_type())
            .map_err(|_| DirEntryError::MetadataError)
    }

    #[inline]
    #[must_use]
    ///Returns the extension of the file if it has one
    pub fn extension(&self) -> Option<&[u8]> {
        self.file_name().rsplit(|&b| b == b'.').next()
    }

    #[inline]
    #[must_use]
    ///Returns the depth relative to the start directory, this is cost free
    pub const fn depth(&self) -> u8 {
        self.depth
    }

    #[inline]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] //this cant be const clippy be LYING
    pub fn len(&self) -> usize {
        debug_assert!(!self.as_bytes().is_empty());
        self.as_bytes().len()
    }

    #[inline]
    #[must_use]
    ///Returns the name of the file (as bytes)
    pub fn file_name(&self) -> &[u8] {
        unsafe { self.as_bytes().get_unchecked(self.base_len as usize..) }
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
    pub const fn base_len(&self) -> u16 {
        self.base_len
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::missing_const_for_fn)] //this cant be const clippy be LYING
    ///returns the path as a &str
    ///this is safe because path is always valid utf8
    ///(because unix paths are always valid utf8)
    pub fn as_str(&self) -> Result<&str> {
        std::str::from_utf8(self.as_bytes()).map_err(DirEntryError::Utf8Error)
    }

    #[inline]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    /// Returns the path as a &str without checking if it is valid UTF-8.
    ///
    /// # Safety
    /// The caller must ensure that the bytes in `self.path` form valid UTF-8.
    #[allow(clippy::missing_panics_doc)]
    pub unsafe fn as_str_unchecked(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(self.as_bytes()) }
    }
    #[inline]
    #[must_use]
    ///Returns the path as a    `Cow<str>`
    pub fn as_str_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(self.as_bytes())
    }

    #[inline]
    #[must_use]
    #[allow(clippy::transmute_ptr_to_ptr)]
    ///Minimal cost conversion  to `OsStr`
    pub fn as_os_str(&self) -> &OsStr {
        // this is safe because the bytes are always valid utf8, same represetation on linux
        unsafe { std::mem::transmute(self.as_bytes()) }
    }

    #[inline]
    #[must_use]
    ///checks extension case insensitively for extension
    pub fn matches_extension(&self, ext: &[u8]) -> bool {
        self.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    #[inline]
    #[must_use]
    ///converts the path (bytes) into an owned path
    pub fn into_path(&self) -> PathBuf {
        PathBuf::from(self.as_os_str())
    }

    #[inline]
    #[must_use]
    ///checks if the file exists, this, makes a syscall
    pub fn exists(&self) -> bool {
        unsafe { self.as_bytes().as_cstr_ptr(|ptr| access(ptr, F_OK)) == 0 }
    }

    #[inline]
    #[must_use]
    ///checks if the file is hidden eg .gitignore
    pub fn is_hidden(&self) -> bool {
        unsafe { *self.as_bytes().get_unchecked(self.base_len as usize) == b'.' }
    }
    #[inline]
    #[must_use]
    ///returns the directory name of the file (as bytes) or failing that (/ is problematic) will return the full path,
    pub fn dirname(&self) -> &[u8] {
        unsafe {
            self.as_bytes() //this is why we store the baseline, to check this and is hidden as babove, its very useful and cheap
                .get_unchecked(..self.base_len as usize - 1)
                .rsplit(|&b| b == b'/')
                .next()
                .unwrap_or(self.as_bytes())
        }
    }

    #[inline]
    #[must_use]
    ///returns the parent directory of the file (as bytes)
    pub fn parent(&self) -> &[u8] {
        unsafe {
            self.as_bytes()
                .get_unchecked(..std::cmp::max(self.base_len as usize - 1, 1))
        }

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
            base_len: get_baselen(path_ref),
        })
    }

    /// Returns an iterator over the directory entries using `readdir64` as opposed to `getdents`, this uses a higher level api
    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn readdir(&self) -> Result<impl Iterator<Item = Self> + '_> {
        DirIter::new(self)
    }

    /// Get file size in bytes
    #[inline]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    pub fn size(&self) -> Result<u64> {
        self.get_stat().map(|s| s.st_size as u64)
    }

    /// Get last modification time, this will be more useful when I implement filters for it.
    #[inline]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    pub fn modified_time(&self) -> Result<SystemTime> {
        self.get_stat().and_then(|s| {
            unix_time_to_system_time(s.st_mtime, s.st_mtime_nsec as i32)
                .map_err(|_| DirEntryError::TimeError)
        })
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    #[allow(clippy::cast_possible_wrap)]
    ///`getdents` is an iterator over fd,where each consequent index is a directory entry.
    /// This function is a low-level syscall wrapper that reads directory entries.
    /// It returns an iterator that yields `DirEntry` objects.
    /// This differs from my `as_iter` impl, which uses libc's `readdir64`, this uses `libc::syscall(SYS_getdents64.....)`
    /// which in theory allows it to be offered turned parameters, ie by purposeley restriction the depth,
    ///  you can likely make the stack copies extremely cheap
    /// EG I use a ~4.1k buffer, which is about close to the max size for most dirents, meaning few will require more than one.
    /// but in actuality, i should/might parameterise this to allow that, i mean its trivial, its about 10 lines in total.
    pub fn getdents(&self) -> Result<impl Iterator<Item = Self>> {
        let dir_path = self.as_bytes();
        let fd = dir_path
            .as_cstr_ptr(|ptr| unsafe { open(ptr, O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) });
        //let fd=unsafe{open_asm(dir_path)};
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
        })
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    #[allow(clippy::cast_possible_wrap)]
    ///`getdents_filter` is an iterator over fd,where each consequent index is a directory entry.
    /// This function is a low-level syscall wrapper that reads directory entries.
    /// It returns an iterator that yields `DirEntry` objects.
    /// This differs from my `as_iter` impl, which uses libc's `readdir64`, this uses `libc::syscall(SYS_getdents64.....)`
    ///this differs from `getdents` in that it allows you to filter the entries by a function.
    /// so it avoids a lot of unnecessary allocations and copies :)
    pub fn getdents_filter(
        &self,
        func: fn(&[u8], usize, u8) -> bool,
    ) -> Result<impl Iterator<Item = Self>> {
        let dir_path = self.as_bytes();
        let fd = dir_path .as_cstr_ptr(|ptr| unsafe { open(ptr, O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) });
        //alternatively syntaxes I made.
        //let fd= unsafe{ open(cstr_n!(dir_path,256),O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) };
        //let fd= unsafe{ open(cstr!(dir_path),O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) };
       // let fd=unsafe{open_asm(dir_path)};

        if fd < 0 {
            return Err(Error::last_os_error().into());
        }

        let mut path_buffer = PathBuffer::new(); // buffer for the path, this is used(the pointer is mutated) to construct the full path of the entry, this is actually
        //a uninitialised buffer, which is then initialised with the directory path
        let mut path_len = dir_path.len();
        init_path_buffer_syscall!(path_buffer, path_len, dir_path, self); // initialise the path buffer with the directory path

        Ok(DirEntryIteratorFilter {
            fd,
            buffer: SyscallBuffer::new(),
            path_buffer,
            base_path_len: path_len as _,
            parent_depth: self.depth,
            offset: 0,
            remaining_bytes: 0,
            filter_func: func,
        })
    }
}

///Iterator for directory entries using getdents syscall
pub struct DirEntryIterator {
    pub(crate) fd: i32, //fd, this is the file descriptor of the directory we are reading from, it is used to read the directory entries via syscall
    pub(crate) buffer: SyscallBuffer, // buffer for the directory entries, this is used to read the directory entries from the file descriptor via syscall, it is 4.3k bytes~ish
    pub(crate) path_buffer: PathBuffer, // buffer for the path, this is used to construct the full path of the entry, this is reused for each entry
    pub(crate) base_path_len: u16, // base path length, this is the length of the path up to and including the last slash
    pub(crate) parent_depth: u8, // depth of the parent directory, this is used to calculate the depth of the child entries
    pub(crate) offset: usize, // offset in the buffer, this is used to keep track of where we are in the buffer
    pub(crate) remaining_bytes: i64, // remaining bytes in the buffer, this is used to keep track of how many bytes are left to read
}

impl Drop for DirEntryIterator {
    /// Drops the iterator, closing the file descriptor.
    /// we need to close the file descriptor when the iterator is dropped to avoid resource leaks.
    /// basically you can only have X number of file descriptors open at once, so we need to close them when we are done.
    #[inline]
    fn drop(&mut self) {
        unsafe { close(self.fd) };
        //unsafe { close_asm(self.fd) };
    }
}

impl Iterator for DirEntryIterator {
    type Item = DirEntry;
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

                let (name_ptr, d_type, reclen, inode): (*const u8, u8, usize, u64) = unsafe {
                    (
                        offset_ptr!(d, d_name).cast(),
                        *offset_ptr!(d, d_type),
                        *offset_ptr!(d, d_reclen) as _,
                        *offset_ptr!(d, d_ino),
                    )
                }; //ideally compiler optimises this to a single load, but it is not guaranteed, so we do it manually.

                self.offset += reclen; //index to next entry, so when we call next again, we will get the next entry in the buffer

                // skip entries that are not valid or are dot entries
                skip_dot_entries!(d_type, name_ptr); //requiring d_type is just a niche optimisation, it allows us not to do 'as many' pointer checks

                let full_path = unsafe { construct_path!(self, name_ptr) }; //a macro that constructs it, the full details are a bit lengthy
                //but essentially its null initialised buffer, copy the starting path (+an additional slash if needed) and copy name of entry
                //this is probably the cheapest way to do it, as it avoids unnecessary allocations and copies.

                let entry = DirEntry {
                    path: full_path.into(),
                    file_type: FileType::from_dtype_fallback(d_type, full_path), //if d_type is unknown fallback to lstat otherwise we get for freeeeeeeee
                    inode,
                    depth: self.parent_depth + 1, // increment depth for child entries
                    base_len: self.base_path_len,
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

pub struct DirEntryIteratorFilter {
    pub(crate) fd: i32, //fd, this is the file descriptor of the directory we are reading from, it is used to read the directory entries via syscall
    pub(crate) buffer: SyscallBuffer, // buffer for the directory entries, this is used to read the directory entries from the file descriptor via syscall, it is 4.3k bytes~ish
    pub(crate) path_buffer: PathBuffer, // buffer for the path, this is used to construct the full path of the entry, this is reused for each entry
    pub(crate) base_path_len: u16, // base path length, this is the length of the path up to and including the last slash
    pub(crate) parent_depth: u8, // depth of the parent directory, this is used to calculate the depth of the child entries
    pub(crate) offset: usize, // offset in the buffer, this is used to keep track of where we are in the buffer
    pub(crate) remaining_bytes: i64, // remaining bytes in the buffer, this is used to keep track of how many bytes are left to read
    pub(crate) filter_func: fn(&[u8], usize, u8) -> bool, // filter function, this is used to filter the entries based on the provided function
                                                          //mainly the arguments would be full path,depth,filetype, this is a shoddy implementation but im testing waters.
}

impl Drop for DirEntryIteratorFilter {
    /// Drops the iterator, closing the file descriptor.
    /// same as above, we need to close the file descriptor when the iterator is dropped to avoid resource leaks.
    #[inline]
    fn drop(&mut self) {
        unsafe { close(self.fd) };
    }
}

impl Iterator for DirEntryIteratorFilter {
    type Item = DirEntry;
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

                let (name_ptr, d_type, reclen, inode): (*const u8, u8, usize, u64) = unsafe {
                    (
                        offset_ptr!(d, d_name).cast(),
                        *offset_ptr!(d, d_type),
                        *offset_ptr!(d, d_reclen) as _,
                        *offset_ptr!(d, d_ino),
                    )
                }; //ideally compiler optimises this to a single load, but it is not guaranteed, so we do it manually.

                self.offset += reclen; //index to next entry, so when we call next again, we will get the next entry in the buffer

                // skip entries that are not valid or are dot entries
                skip_dot_entries!(d_type, name_ptr); //requiring d_type is just a niche optimisation, it allows us not to do 'as many' pointer checks

                let full_path = unsafe { construct_path!(self, name_ptr) }; //a macro that constructs it, the full details are a bit lengthy
                //but essentially its null initialised buffer, copy the starting path (+an additional slash if needed) and copy name of entry
                //this is probably the cheapest way to do it, as it avoids unnecessary allocations and copies.

                let depth = self.parent_depth + 1; // increment depth for child entries

                let file_type = FileType::from_dtype_fallback(d_type, full_path); //if d_type is unknown fallback to lstat otherwise we get for freeeeeeeee

                // apply the filter function to the entry
                //ive had to map the filetype to a value, it's mapped to libc dirent dtype values, this is temporary
                //while i look at implementing a decent state machine for this
                if !(self.filter_func)(full_path, depth as usize, file_type.d_type_value()) {
                    //if the entry does not match the filter, skip it
                    continue;
                }

                let entry = DirEntry {
                    path: full_path.into(),
                    file_type, //if d_type is unknown fallback to lstat otherwise we get for freeeeeeeee
                    inode,
                    depth,
                    base_len: self.base_path_len,
                };

                return Some(entry);
            }

            // prefetch the next buffer content before reading
            #[cfg(target_arch = "x86_64")]
            prefetch_next_buffer!(self);

            // check remaining bytes
            self.remaining_bytes = unsafe { self.buffer.getdents64(self.fd) };
            self.offset = 0;

            if self.remaining_bytes <= 0 {
                // If no more entries, return None,
                return None;
            }
        }
    }
}
