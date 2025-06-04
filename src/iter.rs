#![allow(clippy::cast_possible_wrap)]
#[allow(unused_imports)]
use crate::{
    BytesToCstrPointer, DirEntry, DirEntryError as Error, FileType, PathBuffer, Result,
    SyscallBuffer, copy_name_to_buffer, cstr, init_path_buffer_readdir, offset_ptr,
    skip_dot_entries, strlen_asm,
};
use libc::{DIR, closedir, opendir, readdir64};

#[derive(Debug)]
/// An iterator over directory entries from readdir64 via libc
pub struct DirIter {
    dir: *mut DIR,
    buffer: PathBuffer,
    base_len: u16,
    depth: u8,
    error: Option<Error>,
}

impl DirIter {
    #[inline]
    pub const fn as_mut_ptr(&mut self) -> *mut u8 {
        // This function is used to get a mutable pointer to the internal buffer.
        // It is useful for operations that require direct access to the buffer.
        self.buffer.as_mut_ptr()
    }

    #[inline]
    #[allow(clippy::cast_lossless)]
    #[allow(clippy::cast_possible_truncation)]
    pub fn new(dir_path: &DirEntry) -> Result<Self> {
        let dirp = dir_path.as_bytes();
        let dir = dirp.as_cstr_ptr(|ptr| unsafe { opendir(ptr) });
        //let dir=unsafe{opendir(cstr!(dirp))};
        //alternatively this also works if you dont like closures :)

        if dir.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut buffer = PathBuffer::new();
        let base_len = init_path_buffer_readdir!(dir_path, buffer); //0 cost macro to construct the buffer in the way we want.

        Ok(Self {
            dir,
            buffer,
            base_len: base_len as _,
            depth: dir_path.depth(),
            error: None,
        })
    }
}

impl Iterator for DirIter {
    type Item = DirEntry;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.error.is_some() {
            return None;
        }

        loop {
            let entry = unsafe { readdir64(self.dir) };
            if entry.is_null() {
                return None;
            }
            let name_file: *const u8 = unsafe { offset_ptr!(entry, d_name).cast() };
            let dir_info: u8 = unsafe { *offset_ptr!(entry, d_type).cast() };
            skip_dot_entries!(dir_info, name_file);
            let total_path_len = copy_name_to_buffer!(self, name_file);
            let full_path = unsafe { self.buffer.get_unchecked_mut(..total_path_len) };
            return Some(DirEntry {
                path: full_path.into(),
                file_type: FileType::from_dtype_fallback(dir_info, full_path),
                inode: unsafe { *offset_ptr!(entry, d_ino) },
                depth: self.depth + 1,
                base_len: self.base_len,
            });
        }
    }
}
impl Drop for DirIter {
    fn drop(&mut self) {
        if !self.dir.is_null() {
            unsafe { closedir(self.dir) };
        }
    }
}
