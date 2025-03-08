use libc::{
    access, c_char, close, dirent64, open, syscall, SYS_getdents64, DT_BLK, DT_CHR, DT_DIR,
    DT_FIFO, DT_LNK, DT_REG, DT_SOCK, DT_UNKNOWN, O_RDONLY, PATH_MAX, X_OK,
};

use std::{
    cell::RefCell,
    ffi::OsStr,
    fmt,
    io::{self, Error},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use memchr::{memchr, memchr_iter, memrchr};

const BUFFER_SIZE: usize = 512 * 4;

#[repr(C, align(8))]
struct AlignedBuffer {
    data: [u8; BUFFER_SIZE],
}

#[derive(Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
//it is not excessive, it is a necessary evil(we get this info for free)
//i could add more fields but the struct is 24 bytes so its aligned to memory
//and the fields are used in the filter method
pub struct DirEntry {
    pub path: Box<[u8]>,
    pub(crate) is_dir: bool,
    pub(crate) is_symlink: bool,
    pub(crate) is_regular_file: bool,
    pub(crate) is_fifo: bool,
    pub(crate) is_char: bool,
    pub(crate) is_block: bool,
    pub(crate) is_socket: bool,
    pub(crate) is_unknown: bool,
}

thread_local! {
    static PATH_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(4096));
}

impl fmt::Display for DirEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl DirEntry {
    #[inline(always)]
    #[allow(clippy::inline_always)]
    #[must_use]
    ///costly check for executables
    pub fn is_executable(&self) -> bool {
        if !self.is_regular_file {
            return false;
        }
        let file_path = &self.path;
        //i have to do this here because i would lose the reference to the buffer
        let mut c_path_buf = [0u8; PATH_MAX as usize];
        c_path_buf[..file_path.len()].copy_from_slice(file_path);
        c_path_buf[file_path.len()] = 0;

        unsafe {
            // x_ok checks for execute permission
            access(c_path_buf.as_ptr().cast::<c_char>(), X_OK) == 0
        }
    }
    ///cost free check for block devices
    #[must_use]
    pub const fn is_block_device(&self) -> bool {
        self.is_block
    }
    ///Cost free check for character devices
    #[must_use]
    pub const fn is_char_device(&self) -> bool {
        self.is_char
    }
    ///Cost free check for fifos
    #[must_use]
    pub const fn is_fifo(&self) -> bool {
        self.is_fifo
    }
    ///Cost free check for sockets
    #[must_use]
    pub const fn is_socket(&self) -> bool {
        self.is_socket
    }
    ///Cost free check for regular files
    #[must_use]
    pub const fn is_regular_file(&self) -> bool {
        self.is_regular_file
    }
    ///Cost free check for directories
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        self.is_dir
    }
    ///cost free check for block devices
    #[must_use]
    pub const fn is_char(&self) -> bool {
        self.is_char
    }

    ///cost free check for unknown file types
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        self.is_unknown
    }

    ///cost free check for symlinks
    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        self.is_symlink
    }
    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[allow(clippy::inline_always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        if self.is_regular_file {
            // for files, check if size is zero without loading all metadata
            self.metadata().is_ok_and(|meta| meta.len() == 0)
        } else if self.is_dir {
            //  efficient directory check - just get the first entry
            std::fs::read_dir(self.as_path()).is_ok_and(|mut entries| entries.next().is_none())
        } else {
            // special files like devices, sockets, etc.
            false
        }
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]
    pub fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
        std::fs::metadata(self.as_os_str())
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(self.as_os_str())
    }
    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]
    pub fn file_type(&self) -> io::Result<std::fs::FileType> {
        //  can't directly create a std::fs::FileType,
        // we need to make a system call to get it
        std::fs::symlink_metadata(self.as_path()).map(|m| m.file_type())
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    ///returns the extension of the file if it has one
    pub fn extension(&self) -> Option<&[u8]> {
        self.file_name().rsplit(|&b| b == b'.').next()
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    ///returns the depth of the file in the directory tree (0 for root)
    pub fn depth(&self) -> usize {
        let count = memchr_iter(b'/', &self.path).count();

        if !self.path.is_empty() && self.path[0] == b'/' {
            count.saturating_sub(1)
        } else {
            count
        }
    }
    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn file_name(&self) -> &[u8] {
        memrchr(b'/', &self.path).map_or(&self.path, |pos| &self.path[pos + 1..])
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn filter<F>(&self, f: F) -> bool
    where
        F: Fn(&Self) -> bool,
    {
        f(self)
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(&self.path) }
        //this is safe because path is always valid utf8
        //(because unix paths are always valid utf8)
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn as_os_str(&self) -> &OsStr {
        OsStr::from_bytes(&self.path)
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn matches_extension(&self, ext: &[u8]) -> bool {
        self.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn into_path(&self) -> PathBuf {
        PathBuf::from(self.as_os_str())
    }
    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        let filename = self.file_name();
        !filename.is_empty() && filename[0] == b'.'
    }
    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[allow(clippy::missing_errors_doc)]
    pub fn new(dir_path: &[u8]) -> io::Result<Vec<Self>> {
        //i have to do this here because i would lose the reference to the buffer
        //and i would have to reallocate it in the closure
        let mut c_path_buf = [0u8; PATH_MAX as usize];
        c_path_buf[..dir_path.len()].copy_from_slice(dir_path);
        c_path_buf[dir_path.len()] = 0;

        //this is safe because we are directly converting the
        //path to a c string and passing it to the syscall
        //and the syscall is not modifying the path
        //and the path is not being used after the syscall
        //so there is no chance of use after free
        let fd = unsafe { open(c_path_buf.as_ptr().cast(), O_RDONLY) };
        if fd < 0 {
            return Err(Error::last_os_error());
        }

        //heuristic to reduce the number of allocations
        //this is not a perfect heuristic but it should work for most cases
        //eg on my pc theres only 1 file per directory on average
        let mut entries: Vec<Self> = Vec::with_capacity(4);
        let needs_slash = dir_path != b"/";

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
                    #[allow(clippy::cast_ptr_alignment)]
                    #[allow(clippy::cast_possible_truncation)]
                    //syscall is safe because we are passing a valid fd
                    //and a valid buffer
                    syscall(
                        SYS_getdents64,
                        fd,
                        buffer.data.as_mut_ptr().cast::<dirent64>(),
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
                    //this is safe because we are not modifying the buffer
                    //and the buffer is valid for the lifetime of the loop
                    let d = unsafe { &*(buffer.data.as_ptr().add(offset).cast::<dirent64>()) };

                    let name_end = unsafe {
                        memchr(0, std::slice::from_raw_parts(d.d_name.as_ptr().cast(), 256))
                            .unwrap_or(256)
                    };

                    //this is safe because we are not modifying the buffer
                    //and the buffer is valid for the lifetime of the loop

                    let name_bytes =
                        unsafe { std::slice::from_raw_parts(d.d_name.as_ptr().cast(), name_end) };

                    if name_bytes == b"." || name_bytes == b".." {
                        offset += d.d_reclen as usize;
                        continue;
                    }

                    buf.truncate(base_len);
                    buf.extend_from_slice(name_bytes);

                    entries.push(Self {
                        path: Box::from(buf.as_slice()),
                        is_dir: d.d_type == DT_DIR, //i would explain theres but its trivial lol.
                        is_symlink: d.d_type == DT_LNK,
                        is_regular_file: d.d_type == DT_REG,
                        is_fifo: d.d_type == DT_FIFO,
                        is_char: d.d_type == DT_CHR,
                        is_block: d.d_type == DT_BLK,
                        is_socket: d.d_type == DT_SOCK,
                        is_unknown: d.d_type == DT_UNKNOWN,
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
