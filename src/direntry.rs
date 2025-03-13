#![allow(clippy::inline_always)]
use libc::{access, c_char, close, dirent64, open, syscall, SYS_getdents64, O_RDONLY, X_OK};

use slimmer_box::SlimmerBox;

use std::{
    cell::RefCell,
    ffi::OsStr,
    fmt,
    io::{self, Error},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    slice,
};

use crate::filetype::FileType;
use crate::pointer_conversion::PointerUtils;

use memchr::{memchr, memrchr};

const BUFFER_SIZE: usize = 512 * 4;

#[repr(C, align(8))]
struct AlignedBuffer {
    data: [u8; BUFFER_SIZE],
}

#[derive(Clone)]
pub struct DirEntry {
    pub path: SlimmerBox<[u8]>, //12 bytes
    pub file_type: FileType,    //1 byte
    pub(crate) inode: u64,             //8 bytes
    pub(crate) depth:u16,            //2 bytes    
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

    ///cost free check for character devices
    #[inline(always)]
    #[must_use]
    pub const fn is_char(&self) -> bool {
        matches!(self.file_type, FileType::CharDevice)
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
        // let myitem:u16=65;

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
    #[allow(clippy::missing_errors_doc)]
    pub fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
        std::fs::metadata(self.as_os_str())
    }

    #[inline(always)]
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(self.as_os_str())
    }

    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]
    pub fn file_type(&self) -> io::Result<std::fs::FileType> {
        //  can't directly create a std::fs::FileType,
        // we need to make a system call to get it
        std::fs::symlink_metadata(self.as_path()).map(|m| m.file_type())
    }

    #[inline(always)]
    #[must_use]
    ///returns the extension of the file if it has one
    pub fn extension(&self) -> Option<&[u8]> {
        self.file_name().rsplit(|&b| b == b'.').next()
    }

    #[inline(always)]
    #[must_use]
    ///returns the depth relative to the start directory, this is cost free
    pub const  fn depth(&self) -> u16 {
        self.depth
    }

    #[inline(always)]
    #[must_use]
    ///returns the name of the file
    /// failing to do so, it returns the whole path
    pub fn file_name(&self) -> &[u8] {
        memrchr(b'/', &self.path).map_or(&self.path, |pos| &self.path[pos + 1..])
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
    pub fn as_str_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.path)
    }

    #[inline(always)]
    #[must_use]
    ///minimal cost conversion (lowest cost)
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
    ///converts the path string into an owned path
    pub fn into_path(&self) -> PathBuf {
        PathBuf::from(self.as_os_str())
    }

    #[inline(always)]
    #[must_use]
    ///checks if the file is hidden eg .gitignore
    pub fn is_hidden(&self) -> bool {
        let filename = self.file_name();
        !filename.is_empty() && filename[0] == b'.'
    }

    #[must_use]
    #[inline(always)]
    ///creates a new `DirEntry` from a path
    pub fn new<T: AsRef<OsStr>>(path: T) -> Self {
        let path_ref = path.as_ref();

        Self {
            path: SlimmerBox::new(path_ref.as_bytes()),
            file_type: FileType::from_path(path_ref),
            inode: std::fs::symlink_metadata(path_ref).map_or(0, |meta| meta.ino()), //expensive, not a fan.
            depth:0,
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
            if needs_slash && !dir_path.ends_with(b"/") {
                buf.push(b'/');
            }
            let base_len = buf.len();

            loop {
                let nread = unsafe {
                    #[allow(clippy::cast_possible_truncation)]
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
                #[allow(clippy::cast_possible_truncation)]
                #[allow(clippy::cast_sign_loss)]
                while offset < nread as usize {
                    #[allow(clippy::cast_ptr_alignment)]
                    //we need to cast as dirent to access the relevant fields.
                    //this is safe because we are not modifying the buffer
                    //and the buffer is valid for the lifetime of the loop
                    let d = unsafe { &*(buffer.data.as_ptr().add(offset).cast::<dirent64>()) };

                    let name_end = unsafe {
                        memchr(0, slice::from_raw_parts(d.d_name.as_ptr().cast(), 256))
                            .unwrap_or(256)
                    };

                    //this is safe because we are not modifying the buffer
                    //and the buffer is valid for the lifetime of the loop

                    let name_bytes =
                        unsafe { slice::from_raw_parts(d.d_name.as_ptr().cast(), name_end) };

                    if name_bytes == b"." || name_bytes == b".." {
                        offset += d.d_reclen as usize;
                        continue;
                    }

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
