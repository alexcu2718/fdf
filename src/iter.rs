#![allow(clippy::cast_possible_wrap)]
#[allow(unused_imports)]
use crate::{
    BytePath, DirEntry, DirEntryError as Error, FileType, PathBuffer, Result,
    SyscallBuffer, copy_name_to_buffer, cstr, init_path_buffer_readdir, offset_ptr,get_dirent_vals,
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
        let mut buffer = PathBuffer::new(); //
        //we know it won't be greater than u16::MAX because we limit the path
        let base_len: u16 = init_path_buffer_readdir!(dir_path, buffer) as _; //0 cost macro to construct the buffer in the way we want.
        // The base_len is the length of the path up to the directory being read.
        //mutate the buffer to contain the path up to the directory being read.
        // This is used to avoid copying the path every time we read a directory entry.
        //i concede this isn't the clearest and ill tidy it up un future.

        Ok(Self {
            dir,
            buffer,
            base_len,
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

            let (name_file, dir_info, inode):(*const u8,u8,u64)=get_dirent_vals!(@minimal entry);
            // let (name_file, dir_info, inode,reclen):(*const u8,u8,u64,usize)=get_dirent_vals!(entry); //<-more efficient version to test.
            //*const u8 d8 u64, we dont need reclen, hence the @minimal tag */
            //however, reclen can be used to skip dot entries, because filtering on reclen==24 and checking dtype then pointer check.
            //we mustn't forget that this is an extremely hot loop, so avoiding as much calculation is ideal.
            skip_dot_entries!(dir_info, name_file);
            //skip_dot_entries!(dir_info, name_file, reclen);< -this is the more efficient version, but it requires reclen to be passed in.
            let total_path_len = copy_name_to_buffer!(self, name_file);
            let full_path = unsafe { self.buffer.get_unchecked_mut(..total_path_len) };
            return Some(DirEntry {
                path: full_path.into(),
                file_type: FileType::from_dtype_fallback(dir_info, full_path),
                inode,
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
