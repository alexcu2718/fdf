#![allow(clippy::inline_always)]
#![allow(clippy::cast_ptr_alignment)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
use libc::{access,lstat, c_char, close, dirent64, open,stat, syscall, SYS_getdents64, O_RDONLY,
     X_OK,R_OK,W_OK,F_OK};


use memchr::{memchr, memrchr};
use slimmer_box::SlimmerBox;

use std::{
    cell::RefCell,
    ffi::OsStr,
    fmt,
    io::{self, Error},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    slice,
    time::{Duration, SystemTime, UNIX_EPOCH},
    mem::MaybeUninit
};

use crate::filetype::FileType;
use crate::pointer_conversion::PointerUtils;

const BUFFER_SIZE: usize = 512 * 4;

#[repr(C, align(8))]
struct AlignedBuffer {
    data: [u8; BUFFER_SIZE],
}

#[derive( Clone,PartialEq,  Eq,PartialOrd, Ord, Hash)]
pub struct DirEntry {
    pub path: SlimmerBox<[u8]>, //12 bytes
    pub file_type: FileType,    //1 byte
    pub(crate) inode: u64,             //8 bytes
    pub(crate) depth:u16,            //2 bytes    , this is a max of 65535 directories deep, it's also 2 bytes so keeps struct below 24bytes.
                                //total 23 bytes
                                //1 bytes padding, possible uses? not sure.
}

thread_local! {
    static PATH_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(4096));
}

impl fmt::Display for DirEntry {
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

impl From<&str> for DirEntry {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<&OsStr> for DirEntry {
    fn from(s: &OsStr) -> Self {
        Self::new(s)
    }
}

impl From<&Path> for DirEntry {
    fn from(s: &Path) -> Self {
        Self::new(s)
    }
}

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
    pub const  fn depth(&self) -> u16 {
        self.depth
    }

    #[inline(always)]
    #[must_use]
    ///Returns the name of the file (as bytes)
    /// failing to do so, it returns the whole path
    pub fn file_name(&self) -> &[u8] {
        let path=self.path.as_ref();
        memrchr(b'/', path).map_or(path, |pos| &path[pos + 1..])
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
        Some(&path[..=parent_end])}
     
        




    #[inline(always)]
    #[must_use]
    fn invalid_entry(bytes: &[u8]) -> Self {
        Self {
            path: SlimmerBox::new(bytes),
            file_type: FileType::Unknown,
            inode: 0,
            depth: 0,
        }
    }
    



    #[must_use]
    #[inline(always)]
    ///Creates a new `DirEntry` from a path
    pub fn new<T: AsRef<OsStr>>(path: T) -> Self {
        let path_ref = path.as_ref().as_bytes();
       


        // get file metadata using lstat (doesn't follow symlinks)
        let mut stat_buf = MaybeUninit::<stat>::uninit();
        let res = unsafe {path_ref.as_cstr_ptr(|filename|lstat(filename, stat_buf.as_mut_ptr())) };

        if res != 0 {
            return Self::invalid_entry(path_ref); //this needs to just return an error but TODO!
        }

        // extract information from successful stat
        let stat = unsafe { stat_buf.assume_init() };
        Self {
            path: SlimmerBox::new(path_ref),
            file_type: FileType::from_mode(stat.st_mode),
            inode: stat.st_ino,
            depth: 0,
        }
    }


    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]
    pub fn read_dir(&self) -> io::Result<Vec<Self>> {
        //open is safe because we are passing a valid path
        //and a valid flag
        //need to compute the cstr pointer in a closure so we dont lose the reference
        let dir_path=&self.path;
        let fd = dir_path.as_cstr_ptr(|ptr| unsafe { open(ptr, O_RDONLY) });
        if fd < 0 {
            return Err(Error::last_os_error());
        }

        //heuristic to reduce the number of allocations
        //this is not a perfect heuristic but it should work for most cases
        //eg on my pc theres only 1 file per directory on average
        let mut entries: Vec<Self> = Vec::with_capacity(4);
        let needs_slash = **dir_path != *b"/";

        PATH_BUFFER.with(|buf_cell| -> io::Result<()> {
            let mut buffer = AlignedBuffer {
                data: [0; BUFFER_SIZE],
            };
            let mut buf = buf_cell.borrow_mut();
            buf.clear();
            buf.extend_from_slice(dir_path);
            //the commented out bit is not necessary because the paths dont end with a slash except for the root directory
            //but this is covered by needs slash.
            if needs_slash /*&& !dir_path.ends_with(b"/")*/ {
                buf.push(b'/');
            }
            let base_len = buf.len();

            loop {
                let nread = unsafe {
                    //syscall is safe because we are passing a valid fd
                    //and a valid buffer
                    syscall(
                        SYS_getdents64,
                        fd,
                        buffer.data.as_mut_ptr().cast::<c_char>(), //this is an i8 pointer but more intuitive to display as a c_char
                        BUFFER_SIZE as u32,
                    )
                };

                if nread <= 0 {
                    if nread < 0 {
                        let err = Error::last_os_error();
                        //close is safe because we are passing a valid fd
                        unsafe { close(fd) };
                        return Err(err);
                    }
                    break;
                }

                let mut offset = 0;
              
                while offset < nread as usize {
                    #[allow(clippy::cast_ptr_alignment)]
                    //we need to cast as dirent to access the relevant fields.
                    //this is safe because we are not modifying the buffer
                    //and the buffer is valid for the lifetime of the loop
                    let d = unsafe { &*(buffer.data.as_ptr().add(offset).cast::<dirent64>()) };

                    
                    let name_ptr = d.d_name.as_ptr().cast::<u8>();

                    let name_end = unsafe {
                        memchr(0, slice::from_raw_parts(name_ptr, 256)).unwrap_or(256)
                    };

                    // Fast check for "." and ".." to skip unnecessary work
                    if name_end <= 2 {
                        let first = unsafe { *name_ptr };
                        if first == b'.' && (name_end == 1 || unsafe { *name_ptr.add(1) } == b'.') {
                            offset += d.d_reclen as usize;
                            continue;
                        }
                    }

                    // create name only  after filtering out "." and ".."
                    let name_bytes = unsafe { slice::from_raw_parts(name_ptr, name_end) };

                
                    buf.truncate(base_len);
                    buf.extend_from_slice(name_bytes);
                    //this is safe because the path is bounded FAR below the limit (something like a few gb)
                    entries.push(Self {
                        path: unsafe { SlimmerBox::new_unchecked(&buf) },
                        file_type: FileType::from_dtype(d.d_type),
                        inode: d.d_ino,
                        depth:self.depth+1,
                    });

                    offset += d.d_reclen as usize;
                }
            }
            Ok(())
        })?;
        //close is safe because we are passing a valid fd
        unsafe { close(fd) };

        Ok(entries)
    }
}




impl DirEntry {
    /// Helper to safely perform stat syscall
    #[inline(always)]
    fn get_stat(&self) -> io::Result<stat> {
        let mut stat_buf = MaybeUninit::<stat>::uninit();
        
        let res = self.path.as_cstr_ptr(|ptr| unsafe {
            stat(ptr, stat_buf.as_mut_ptr())
        });

        if res == 0 {
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Get file size in bytes
    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]//fixing errors later
    pub fn size(&self) -> io::Result<u64> {
        self.get_stat().map(|s| s.st_size as u64)
    }

    /// Get last modification time
    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]//fixing errors later
    pub fn modified_time(&self) -> io::Result<SystemTime> {
        self.get_stat().and_then(|s| {
            let sec = s.st_mtime;
            let nsec = s.st_mtime_nsec as i32;
            unix_time_to_system_time(sec, nsec)
        })
    }

}





/// Convert Unix timestamp (seconds + nanoseconds) to `SystemTime`
#[allow(clippy::missing_errors_doc)]//fixing errors later
fn unix_time_to_system_time(sec: i64, nsec: i32) -> io::Result<SystemTime> {
    let (base, offset) = if sec >= 0 {
        (UNIX_EPOCH, Duration::new(sec as u64, nsec as u32))
    } else {
        let sec_abs = sec.unsigned_abs();
        (UNIX_EPOCH + Duration::new(sec_abs, 0), 
        Duration::from_nanos(nsec as u64))
    };

    base.checked_sub(offset)
        .or_else(|| UNIX_EPOCH.checked_sub(Duration::from_secs(0)))
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidData, 
            "Invalid timestamp value"
        ))
}


