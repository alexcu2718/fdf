#![allow(clippy::inline_always)]
#![allow(clippy::cast_ptr_alignment)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
use libc::{
    access, c_char, close, dirent64, lstat, open, stat, syscall, SYS_getdents64, F_OK, O_RDONLY,
    R_OK, W_OK, X_OK,
};

use memchr::memrchr;
use slimmer_box::SlimmerBox;

use std::{
    cell::RefCell,
    ffi::CStr,
    ffi::OsStr,
    fmt,
    io::{self, Error},
    mem::MaybeUninit,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::filetype::FileType;
use crate::pointer_conversion::PointerUtils;

const BUFFER_SIZE: usize = 512 * 4;

#[derive(Debug,Clone)]
pub enum DirEntryError {
    InvalidPath,
    InvalidStat,

}

#[repr(C, align(8))]
struct AlignedBuffer {
    data: [u8; BUFFER_SIZE],
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DirEntry {
    pub path: SlimmerBox<[u8],u16>,     //10 bytes
    pub(crate) file_type: FileType, //1 byte
    pub(crate) inode: u64,          //8 bytes
    pub(crate) depth: u8, //1 bytes    , this is a max of 65535 directories deep, it's also 1 bytes so keeps struct below 24bytes.
    pub(crate) base_len: u8, //1 bytes     , this info is free and helps to get the filename.               
                           //total 21 bytes
                           //4 bytes padding, possible uses? not sure.
}

thread_local! {
    static PATH_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(4096));
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
        &self.path
    }
}

impl AsRef<Path> for DirEntry {
    #[inline(always)]
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
    #[inline(always)]
    #[must_use]
    ///costly check for executables
    pub fn is_executable(&self) -> bool {
        if !self.is_regular_file() {
            return false;
        }

        unsafe {
            // x_ok checks for execute permission
            self.path.as_cstr_ptr(|ptr| access(ptr, X_OK) == 0)
        }
    }

    ///cost free check for block devices
    #[inline(always)]
    #[must_use]
    pub const fn is_block_device(&self) -> bool {
        matches!(self.file_type, FileType::BlockDevice)
    }

    ///Cost free check for character devices
    #[inline(always)]
    #[must_use]
    pub const fn is_char_device(&self) -> bool {
        matches!(self.file_type, FileType::CharDevice)
    }

    ///Cost free check for fifos
    #[inline(always)]
    #[must_use]
    pub const fn is_fifo(&self) -> bool {
        matches!(self.file_type, FileType::Fifo)
    }

    ///Cost free check for sockets
    #[inline(always)]
    #[must_use]
    pub const fn is_socket(&self) -> bool {
        matches!(self.file_type, FileType::Socket)
    }

    ///Cost free check for regular files
    #[inline(always)]
    #[must_use]
    pub const fn is_regular_file(&self) -> bool {
        matches!(self.file_type, FileType::RegularFile)
    }

    ///Cost free check for directories
    #[inline(always)]
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        matches!(self.file_type, FileType::Directory)
    }

    ///cost free check for unknown file types
    #[inline(always)]
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self.file_type, FileType::Unknown)
    }

    ///cost free check for symlinks
    #[inline(always)]
    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        matches!(self.file_type, FileType::Symlink)
    }

    #[inline(always)]
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

    #[inline(always)]
    #[must_use]
    ///somewhatcostly check for readable files
    pub fn is_readable(&self) -> bool {
        unsafe { self.path.as_cstr_ptr(|ptr| access(ptr, R_OK)) == 0 }
    }

    #[inline(always)]
    #[must_use]
    ///somewhat costly check for writable files
    pub fn is_writable(&self) -> bool {
        unsafe { self.path.as_cstr_ptr(|ptr| access(ptr, W_OK)) == 0 }
    }

    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]
    pub fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
        std::fs::metadata(self.as_os_str())
    }
    #[inline(always)]
    #[allow(clippy::missing_const_for_fn)] //this cant be const clippy be LYING AGAIN
    #[must_use]
    ///Cost free conversion to bytes (because it is already is bytes)
    pub fn as_bytes(&self) -> &[u8] {
        &self.path
    }

    #[inline(always)]
    #[must_use]
    ///checks if the path is absolute
    pub fn is_absolute(&self) -> bool {
        self.path.first() == Some(&b'/')
    }

    #[inline(always)]
    pub fn components(&self) -> impl Iterator<Item = &[u8]> {
        self.path.split(|&b| b == b'/').filter(|s| !s.is_empty())
    }

    #[inline(always)]
    #[must_use]
    ///Low cost free conversion to a `Path`
    pub fn as_path(&self) -> &Path {
        Path::new(self.as_os_str())
    }

    #[inline(always)]
    #[must_use]
    ///returns the file type of the file (eg directory, regular file, etc)
    pub const fn file_type(&self) -> FileType {
        self.file_type
    }

    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]
    ///Costly conversion to a `std::fs::FileType`
    pub fn to_std_file_type(&self) -> io::Result<std::fs::FileType> {
        //  can't directly create a std::fs::FileType,
        // we need to make a system call to get it
        std::fs::symlink_metadata(self.as_path()).map(|m| m.file_type())
    }

    #[inline(always)]
    #[must_use]
    ///Returns the extension of the file if it has one
    pub fn extension(&self) -> Option<&[u8]> {
        self.file_name().rsplit(|&b| b == b'.').next()
    }

    #[inline(always)]
    #[must_use]
    ///Returns the depth relative to the start directory, this is cost free
    pub const fn depth(&self) -> u8 {
        self.depth
    }

    #[inline(always)]
    #[must_use]
    ///Returns the name of the file (as bytes)
    pub fn file_name(&self) -> &[u8] {
        &self.path.as_ref()[self.base_len as usize..]
        
    }
    

    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]
    #[must_use]
    ///returns the inode number of the file, rather expensive
    /// i just included it for sake of completeness.
    pub const fn ino(&self) -> u64 {
        self.inode
    }

    #[inline(always)]
    #[must_use]
    pub fn filter<F>(&self, f: F) -> bool
    where
        F: Fn(&Self) -> bool,
    {
        f(self)
    }

    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::missing_const_for_fn)] //this cant be const clippy be LYING
    ///returns the path as a &str
    ///this is safe because path is always valid utf8
    ///(because unix paths are always valid utf8)
    pub fn as_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.path)
    }

    #[inline(always)]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    /// Returns the path as a &str without checking if it is valid UTF-8.
    ///
    /// # Safety
    /// The caller must ensure that the bytes in `self.path` form valid UTF-8.
    pub unsafe fn as_str_unchecked(&self) -> &str {
        std::str::from_utf8_unchecked(&self.path)
    }
    #[inline(always)]
    #[must_use]
    ///Returns the path as a    `Cow<str>`
    pub fn as_str_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.path)
    }

    #[inline(always)]
    #[must_use]
    ///Minimal cost conversion  to `OsStr`
    pub fn as_os_str(&self) -> &OsStr {
        OsStr::from_bytes(&self.path)
    }

    #[inline(always)]
    #[must_use]
    ///checks extension case insensitively for extension
    pub fn matches_extension(&self, ext: &[u8]) -> bool {
        self.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    #[inline(always)]
    #[must_use]
    ///converts the path (bytes) into an owned path
    pub fn into_path(&self) -> PathBuf {
        PathBuf::from(self.as_os_str())
    }

    #[inline(always)]
    #[must_use]
    pub fn exists(&self) -> bool {
        unsafe { self.path.as_cstr_ptr(|ptr| access(ptr, F_OK)) == 0 }
    }

    #[inline(always)]
    #[must_use]
    ///checks if the file is hidden eg .gitignore
    pub fn is_hidden(&self) -> bool {
        let filename = self.file_name();
        !filename.is_empty() && filename[0] == b'.'
    }

    #[inline(always)]
    #[must_use]
    pub fn parent(&self) -> Option<&[u8]> {
        let path = self.path.as_ref();
        let parent_end = memrchr(b'/', path)?;
        Some(&path[..=parent_end])
    }



   

    #[must_use]
    #[inline(always)]
    ///Creates a new `DirEntry` from a path
    pub fn new<T: AsRef<OsStr>>(path: T) -> Result<Self, DirEntryError> {
        let path_os_str = path.as_ref();
        let path_ref=path_os_str.as_bytes();
        // get file metadata using lstat (doesn't follow symlinks)
        let mut stat_buf = MaybeUninit::<stat>::uninit();
        let res =
            unsafe { path_ref.as_cstr_ptr(|filename| lstat(filename, stat_buf.as_mut_ptr())) };

        if res != 0 {
            return Err(DirEntryError::InvalidPath) //this needs to just return an error but TODO!
        }

        // extract information from successful stat
        let stat = unsafe { stat_buf.assume_init() };
        Ok(Self {
            path: SlimmerBox::new(path_ref),
            file_type: FileType::from_mode(stat.st_mode),
            inode: stat.st_ino,
            depth: 0,
            base_len:Path::new(path_os_str).parent().map_or(0,|x|x.as_os_str().as_bytes().len() as u8)
        })
    }

    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]
    pub fn read_dir(&self) -> io::Result<Vec<Self>> {
        let dir_path = &self.path;
        let fd = dir_path.as_cstr_ptr(|ptr| unsafe { open(ptr, O_RDONLY) });
        if fd < 0 {
            return Err(Error::last_os_error());
        }

        let mut entries: Vec<Self> = Vec::with_capacity(4);
        let needs_slash = **dir_path != *b"/";

        PATH_BUFFER.with(|buf_cell| -> io::Result<()> {
            let mut buffer = AlignedBuffer {
                data: [0; BUFFER_SIZE],
            };
            let mut buf = buf_cell.borrow_mut();
            buf.clear();
            buf.extend_from_slice(dir_path);
            if needs_slash {
                buf.push(b'/');
            }
            let base_len = buf.len();

            loop {
                let nread = unsafe {
                    syscall(
                        SYS_getdents64,
                        fd,
                        buffer.data.as_mut_ptr().cast::<c_char>(),
                        BUFFER_SIZE as u32,
                    )
                };

                if nread <= 0 {
                    if nread < 0 {
                        let err = Error::last_os_error();
                        unsafe { close(fd) };
                        return Err(err);
                    }
                    break;
                }

                let mut offset = 0;

                while offset < nread as usize {
                    let d = unsafe { &*(buffer.data.as_ptr().add(offset).cast::<dirent64>()) };

                    // SAFETY: kernel guarantees null-terminated d_name
                    let name_cstr = unsafe { CStr::from_ptr(d.d_name.as_ptr()) };
                    let name_bytes = name_cstr.to_bytes();

                    // fast path check using byte length first
                    if name_bytes.len() <= 2 {
                        match name_bytes {
                            b"." | b".." => {
                                offset += d.d_reclen as usize;
                                continue;
                            }
                            _ => {}
                        }
                    }

                    buf.truncate(base_len);
                    buf.extend_from_slice(name_bytes);

                    entries.push(Self {
                        // SAFETY:
                        //The caller must ensure that slice’s length can fit in a u32.
                        path: unsafe { SlimmerBox::new_unchecked(&buf) },
                        file_type: FileType::from_dtype(d.d_type),
                        inode: d.d_ino,
                        depth: self.depth + 1,
                        base_len:base_len as u8
                    });

                    offset += d.d_reclen as usize;
                }
            }
            Ok(())
        })?;

        unsafe { close(fd) };
        Ok(entries)
    }
}

impl DirEntry {
    /// Helper to safely perform stat syscall
    #[inline(always)]
    fn get_stat(&self) -> io::Result<stat> {
        let mut stat_buf = MaybeUninit::<stat>::uninit();

        let res = self
            .path
            .as_cstr_ptr(|ptr| unsafe { stat(ptr, stat_buf.as_mut_ptr()) });

        if res == 0 {
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Get file size in bytes
    #[inline(always)]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    pub fn size(&self) -> io::Result<u64> {
        self.get_stat().map(|s| s.st_size as u64)
    }

    /// Get last modification time
    #[inline(always)]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    pub fn modified_time(&self) -> io::Result<SystemTime> {
        self.get_stat().and_then(|s| {
            let sec = s.st_mtime;
            let nsec = s.st_mtime_nsec as i32;
            unix_time_to_system_time(sec, nsec)
        })
    }
}

/// Convert Unix timestamp (seconds + nanoseconds) to `SystemTime`
#[allow(clippy::missing_errors_doc)] //fixing errors later
fn unix_time_to_system_time(sec: i64, nsec: i32) -> io::Result<SystemTime> {
    let (base, offset) = if sec >= 0 {
        (UNIX_EPOCH, Duration::new(sec as u64, nsec as u32))
    } else {
        let sec_abs = sec.unsigned_abs();
        (
            UNIX_EPOCH + Duration::new(sec_abs, 0),
            Duration::from_nanos(nsec as u64),
        )
    };

    base.checked_sub(offset)
        .or_else(|| UNIX_EPOCH.checked_sub(Duration::from_secs(0)))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid timestamp value"))
}

