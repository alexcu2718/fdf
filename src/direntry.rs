#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::single_call_fn)]

use libc::{
 F_OK, O_CLOEXEC, O_DIRECTORY, O_NONBLOCK, O_RDONLY, R_OK, W_OK, X_OK,
    access, close, open, strlen,//dirent64
};
#[allow(unused_imports)]
use std::{
    ffi::OsStr,
    fmt,
    io::Error,
    sync::Arc,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::SystemTime,
};

//this is from a wizardy C forum. basically, the final directory name length (256 bytes aka 4096 bits aka path max +final filename length)
//is 256 bytes(CAN DIFFER DEPENDING ON LIBC), so we can use that to calculate the size of the buffer. there should NEVER be anything bigger than the buffer
///check assert in the code below
//c code is offsetof(struct dirent, d_name) + PATH_MAX is enough to one shot.
#[allow(unused_imports)]
use crate::{process_getdents_loop,DirIter, ToStat,//strlen_asm,offset_ptr,skip_dot_entries,
    OsBytes, Result, error::DirEntryError, filetype::FileType,PathBuffer,SyscallBuffer,custom_types_result::SlimOsBytes ,init_path_buffer_syscall,AsU8,
    traits_and_conversions::BytesToCstrPointer, utils::unix_time_to_system_time,utils::get_baselen,//init_path_with_slash
};

//this is a 4k buffer, which is the maximum size of a directory entry on most filesystems
//might change this, who knows?

#[derive(Clone)]
pub struct DirEntry {
    pub(crate) path: SlimOsBytes, //10 bytes,this is basically a box with a much thinner pointer, it's 10 bytes instead of 16.
    pub(crate) file_type: FileType, //1 byte
    pub(crate) inode: u64,    //8 bytes, i may drop this in the future, it's not very useful.
    pub(crate) depth: u8, //1 bytes    , this is a max of 255 directories deep, it's also 1 bytes so keeps struct below 24bytes.
    pub(crate) base_len: u16, //2 bytes     , this info is free and helps to get the filename.its formed by path length until  and including last /.
                              //total 22 bytes
                              //3 bytes padding, possible uses? not sure.

                              //maybe i can add is is_hidden attribute, this is 'free' and can be used to check if the file is hidden.
                              //this is a 1 byte attribute, so it keeps the struct below 24 bytes (22 bytes with that)
                              //other ideas are yet to be made

                              //possible ideas is storing the dirents value, as a direct struct, with handy functions to get the details
                              //except for the name, the filename is easy to grab from the pointer but it's then resolving to form
                              //a full path, the preceeding path is not stored, maybe i can store it in a buffer
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
    pub fn is_empty(&self) -> bool {
        if self.is_regular_file() {
            // for files, check if size is zero without loading all metadata
            //  self.metadata().is_ok_and(|meta| meta.len() == 0)
            self.size().is_ok_and(|size| size == 0)
        } else if self.is_dir() {
            //  efficient directory check - just get the first entry
            // std::fs::read_dir(self.as_path()).is_ok_and(|mut entries| entries.next().is_none())
            self.as_iter()
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

        let ptr = unsafe {
            self.as_bytes()
                .as_cstr_ptr(|cstrpointer| libc::realpath(cstrpointer, std::ptr::null_mut()))
        };
        if ptr.is_null() {
            return Err(Error::last_os_error().into());
        }
        //better to use strlen here because path is likely to be too long to benefit from repne scasb
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

        unsafe { &*std::ptr::slice_from_raw_parts(ptr.cast::<u8>(), strlen(ptr)) }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    ///Converts a path to a proper path, if it is not already
    /// Errors if i've fucked up my pointer shit here lol,not properly tested.
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
    pub fn metadata(&self) -> Result<std::fs::Metadata> {
        std::fs::metadata(self.as_os_str()).map_err(|_| DirEntryError::MetadataError)
    }
    #[inline]
    #[allow(clippy::missing_const_for_fn)] //this cant be const clippy be LYING AGAIN
    #[must_use]
    ///Cost free conversion to bytes (because it is already is bytes)
    pub  fn as_bytes(&self) -> &[u8] {
        self.path.as_bytes()
    }

    #[inline]
    #[must_use]
    ///checks if the path is absolute
    pub fn is_absolute(&self) -> bool {
        self.as_bytes()[0]==b'/'
    }

    #[inline]
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
    ///Minimal cost conversion  to `OsStr`
    pub fn as_os_str(&self) -> &OsStr {
        OsStr::from_bytes(self.as_bytes())
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
    pub fn dirname(&self) -> &[u8] {
       unsafe{self.as_bytes().get_unchecked(..self.base_len as usize - 1)
            .rsplit(|&b| b == b'/')
            .next()
            .unwrap_or(self.as_bytes())}
        //we need to be careful if it's root,im not a fan of this method but eh.
        //theres probably a more elegant way.
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

    /// Returns an iterator over the directory entries.
    /// very unique API compared to my other one, was an experiment.
    /// im not sure about this...its a complex type.
    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn as_iter(&self) -> Result<impl Iterator<Item = Self> + '_> {
        DirIter::new(self)
    }

    /// Get file size in bytes
    #[inline]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    pub fn size(&self) -> Result<u64> {
        self.get_stat().map(|s| s.st_size as u64)
    }

    /// Get last modification time
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
    pub fn read_dir(&self) -> Result<Vec<Self>> {
    let dir_path = self.as_bytes();
    let fd = dir_path.as_cstr_ptr(|ptr| unsafe { open(ptr, O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) });
    //alternatively
    //use crate::cstr;
    //use libc::open;
    //let fd=unsafe{ open(cstr!(dir_path),O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC)};
    if fd < 0 {
        return Err(Error::last_os_error().into());
    }

    let mut entries = Vec::with_capacity(8);//arbitrary number I made up.
    let mut path_buffer = PathBuffer::new();
    let mut path_len = dir_path.len();

    init_path_buffer_syscall!(path_buffer, path_len, dir_path, self);
    
    let mut buffer = SyscallBuffer::new();
    process_getdents_loop!(buffer, fd, entries, path_buffer, path_len, self);

    unsafe { close(fd) };

    Ok(entries)
}

}