#![allow(clippy::cast_ptr_alignment)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
use libc::{
    access, c_char, dirent64, free, realpath, strlen, SYS_getdents64, F_OK, O_RDONLY, R_OK,
    W_OK, X_OK,syscall,close,open,PATH_MAX
};

use std::{
    //intrinsics::likely,
    ffi::OsStr,
    fmt,
    io::Error,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    slice,
    time::SystemTime,
};

pub use crate::utils::{get_baselen,  unix_time_to_system_time};
use crate::{ DirIter, ToStat};
pub use crate::{
    error::DirEntryError, filetype::FileType, traits_and_conversions::BytesToCstrPointer, OsBytes,
    Result,
};

const BUFFER_SIZE: usize = 512 * 8;


//this is a 4k buffer, which is the maximum size of a directory entry on most filesystems

#[repr(C, align(8))]
pub struct AlignedBuffer {
    pub data: [u8; BUFFER_SIZE],
}

#[derive(Clone)]
pub struct DirEntry {
    pub path: OsBytes, //10 bytes,this is basically a box with a much thinner pointer, it's 10 bytes instead of 16.
    pub(crate) file_type: FileType, //1 byte
    pub(crate) inode: u64, //8 bytes, i may drop this in the future, it's not very useful.
    pub(crate) depth: u8, //1 bytes    , this is a max of 65535 directories deep, it's also 1 bytes so keeps struct below 24bytes.
    pub(crate) base_len: u8, //1 bytes     , this info is free and helps to get the filename.its formed by path length until  and including last /.
                             //total 21 bytes
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

/// `DirEntry` is safe to pass from one thread to another, as it's not reference-counted.
unsafe impl Send for DirEntry {}

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
            self.metadata().is_ok_and(|meta| meta.len() == 0)
        } else if self.is_dir() {
            //  efficient directory check - just get the first entry
            std::fs::read_dir(self.as_path()).is_ok_and(|mut entries| entries.next().is_none())
        } else {
            // special files like devices, sockets, etc.
            false
        }
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

        let ptr = unsafe {
            self.as_bytes()
                .as_cstr_ptr(|cstrpointer| realpath(cstrpointer, std::ptr::null_mut()))
        };
        if ptr.is_null() {
            return Err(Error::last_os_error().into());
        }

        let bytes = unsafe { slice::from_raw_parts(ptr.cast::<u8>(), strlen(ptr)) };
        //safe because easily fits in capacity (which is absurdly for our purposes)

        let boxed = Self {
            path: OsBytes::new(bytes),
            file_type: self.file_type,
            inode: self.inode,
            depth: self.depth,
            base_len: (bytes.len() - self.file_name().len()) as u8,
        }; //we need the length up to the filename INCLUDING
           //including for slash, so eg ../hello/etc.txt has total len 16, then its base_len would be 16-7=9bytes
           //so we subtract the filename length from the total length, probably could've been done more elegantly.
           //TBD? not imperative.
        unsafe { free(ptr.cast()) }; //we need to free the pointer AFTER copying the data
                                     // Return the new DirEntry

        Ok(boxed)
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
    pub fn as_bytes(&self) -> &[u8] {
        self.path.as_bytes()
    }

    #[inline]
    #[must_use]
    ///checks if the path is absolute
    pub fn is_absolute(&self) -> bool {
        self.as_bytes().first() == Some(&b'/')
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
        debug_assert!(self.as_bytes().len()>0);
        self.as_bytes().len()
    }

    #[inline]
    #[must_use]
    ///Returns the name of the file (as bytes)
    pub fn file_name(&self) -> &[u8] {
        &self.as_bytes()[self.base_len as usize..]
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
    pub fn filter(&self, func: impl Fn(&Self) -> bool) -> bool {
        func(self)
    }

    #[inline]
    #[must_use]
    ///returns the length of the base path (eg /home/user/ is 6)
    pub const fn base_len(&self) -> u8 {
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
    pub unsafe fn as_str_unchecked(&self) -> &str {
        debug_assert!(self.as_bytes().is_ascii());
        //mouthful!
        debug_assert_eq!(
            DirEntry::new(std::path::PathBuf::from(".").canonicalize().unwrap()).unwrap().as_full_path().unwrap().as_bytes(),
            self.as_bytes()
        );
        std::str::from_utf8_unchecked(self.as_bytes())
    }
    #[inline]
    #[must_use]
    ///Returns the path as a    `Cow<str>`
    pub fn as_str_lossy(&self) -> std::borrow::Cow<'_, str> {
        debug_assert!(self.as_bytes().is_ascii());
        
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
        //self.file_name().first() == Some(&b'.')
        self.as_bytes()[self.base_len as usize] == b'.'
    }
    #[inline]
    #[must_use]
    pub fn dirname(&self) -> &[u8] {


        self.as_bytes()[..self.base_len as usize - 1]
            .rsplit(|&b| b == b'/')
            .next()
            .unwrap_or(self.as_bytes())
        //we need to be careful if it's root,im not a fan of this method but eh.
        //theres probably a more elegant way.
    }

    #[inline]
    #[must_use]
    ///returns the parent directory of the file (as bytes)
    pub fn parent(&self) -> &[u8] {

        
        &self.as_bytes()[..std::cmp::max(self.base_len as usize - 1, 1)]
        
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
    #[inline]
    #[allow(clippy::missing_errors_doc)]
    ///Creates a new `DirEntry` from a path
    pub fn read_dir(&self) -> Result<Vec<Self>> {
        let dir_path = self.as_bytes();
        let fd = dir_path.as_cstr_ptr(|ptr| unsafe { open(ptr, O_RDONLY) });
        if fd < 0 {
            return Err(Error::last_os_error().into());
        }

        let needs_slash = dir_path != b"/";
        let mut entries = Vec::with_capacity(8);

        // use a stack-allocated buffer with `PATH_MAX` size
        let mut path_buffer = [0u8; PATH_MAX as usize];
        let dir_path_len = dir_path.len();

        path_buffer[..dir_path_len].copy_from_slice(dir_path);
        let mut path_len = dir_path_len;

        // add trailing slash if needed
        if needs_slash {
            path_buffer[path_len] = b'/';
            path_len += 1;
        }
   

        let mut buffer = AlignedBuffer {
            data: [0; BUFFER_SIZE],
        };
        #[allow(clippy::cast_possible_wrap)]
        loop {
            let nread = unsafe {
                syscall(
                    SYS_getdents64,
                    fd,
                    buffer.data.as_mut_ptr().cast::<c_char>(),
                    BUFFER_SIZE,
                )
            } as isize;

            match nread {
                0 => break, // End of directory
                n if n < 0 => {
                  
                    return Err(Error::last_os_error().into());
                }
                n => {
                    let mut offset = 0;
                    while offset < n as usize {
                        let d = unsafe { &*buffer.data.as_ptr().add(offset).cast::<dirent64>() };
                        let reclen = d.d_reclen as usize;

                        let name_ptr = d.d_name.as_ptr();
                        let name_len = unsafe { strlen(name_ptr) };
                        let name_bytes = unsafe { std::slice::from_raw_parts(name_ptr.cast::<u8>(), name_len) };
                       

                        // Skip . and ..
                        if name_bytes == b"." || name_bytes == b".." {
                            offset += reclen;
                            continue;
                        }

                       

                        let total_len = path_len + name_len as usize;
                        // build full path in buffer
                        path_buffer[path_len..total_len].copy_from_slice(name_bytes);
                        let full_path = &path_buffer[..total_len];

                        // Create path and push entry
                        entries.push(Self {
                            path: full_path.into(), // convert to our type
                            file_type: FileType::from_dtype_fallback(d.d_type, full_path),
                            inode: d.d_ino,
                            depth: self.depth + 1,
                            base_len: path_len as u8,
                        });

                        offset += reclen;
                    }
                }
            }
        }

        unsafe { close(fd) };
        entries.shrink_to_fit();
        Ok(entries)
    }
    /// Returns an iterator over the directory entries.
    /// very unique API compared to my other one, was an experiment.
    /// im not sure about this...its a complex type.
    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn as_iter(&self) -> Result<impl Iterator<Item = Result<Self>> + '_> {
        DirIter::new(self)
    }

    pub fn into_iter(&self) -> impl Iterator<Item = Vec<Self>> {
        self.read_dir().into_iter()
    }

    /// Get file size in bytes
    #[inline]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    pub fn size(&self) -> Result<u64> {
        self.get_stat().map(|s| s.st_size as u64)
    }

    /// Get last modification time
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    pub fn modified_time(&self) -> Result<SystemTime> {
        self.get_stat().and_then(|s| {
            let sec = s.st_mtime;
            let nsec = s.st_mtime_nsec as i32;
            unix_time_to_system_time(sec, nsec).map_err(|_| DirEntryError::TimeError)
        })
    }
}

