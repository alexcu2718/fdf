use libc::{
    access, c_char, close, dirent64, lstat, open, syscall, SYS_getdents64, DT_BLK, DT_CHR, DT_DIR,
    DT_FIFO, DT_REG, DT_SOCK, DT_UNKNOWN, O_RDONLY, PATH_MAX, S_IFLNK, S_IFMT, X_OK,
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
    pub is_dir: bool,
    pub is_unknown: bool,
    pub is_regular_file: bool,
    pub is_fifo: bool,
    pub is_char: bool,
    pub is_block: bool,
    pub is_socket: bool,
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
    pub fn is_executable(&self) -> bool {
        if self.is_unknown || !self.is_regular_file {
            return false;
        }

        // Create a null-terminated path for libc
        let mut path_buf = [0u8; PATH_MAX as usize];
        let path_len = std::cmp::min(self.path.len(), path_buf.len() - 1);
        path_buf[..path_len].copy_from_slice(&self.path[..path_len]);
        path_buf[path_len] = 0; // Null terminator

        unsafe {
            // X_OK (1) checks for execute permission
            access(path_buf.as_ptr().cast::<c_char>(), X_OK) == 0
        }
    }

    #[inline(always)]
    #[allow(clippy::inline_always)]
    #[must_use]
    pub fn is_symlink(&self) -> bool {
        if self.is_regular_file {
            return false;
        }
        // Create a null-terminated path for libc
        let mut path_buf = [0u8; PATH_MAX as usize];
        let path_len = std::cmp::min(self.path.len(), path_buf.len() - 1);
        path_buf[..path_len].copy_from_slice(&self.path[..path_len]);
        path_buf[path_len] = 0;

        unsafe {
            let mut stat_buf: libc::stat = std::mem::zeroed();
            if lstat(path_buf.as_ptr().cast::<c_char>(), &mut stat_buf) == 0 {
                (stat_buf.st_mode & S_IFMT) == S_IFLNK
            } else {
                false
            }
        }
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
        // Since we can't directly create a std::fs::FileType,
        // we need to make a system call to get it
        std::fs::symlink_metadata(self.as_path()).map(|m| m.file_type())
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn extension(&self) -> Option<&[u8]> {
        let filename = self.file_name();
        filename
            .iter()
            .rposition(|&b| b == b'.')
            .map(|pos| &filename[pos + 1..])
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn get_depth(&self) -> usize {
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
        //this assumption is because unix paths are always valid utf8
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    #[must_use]
    pub fn to_filename_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(self.file_name()) }
        //this is safe because file_name() returns a slice of the path which is always valid utf8
        //and we are not modifying the slice in any way
        //this assumption is because unix paths are always valid utf8
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
    pub fn as_os_str_filename(&self) -> &OsStr {
        OsStr::from_bytes(self.file_name())
        //this might be overkill, remove if warranted,
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
            if needs_slash {
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
                        is_unknown: d.d_type == DT_UNKNOWN,
                        is_regular_file: d.d_type == DT_REG,
                        is_fifo: d.d_type == DT_FIFO,
                        is_char: d.d_type == DT_CHR,
                        is_block: d.d_type == DT_BLK,
                        is_socket: d.d_type == DT_SOCK,
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
