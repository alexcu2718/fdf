#![allow(clippy::cast_possible_wrap)]
#[allow(unused_imports)]
use crate::{
    BytePath, DirEntry, DirEntryError as Error, FileType, PathBuffer, Result, SyscallBuffer,
    construct_path, cstr, custom_types_result::BytesStorage, get_dirent_vals,
    init_path_buffer_readdir, offset_ptr, skip_dot_entries,
};
use libc::{DIR, closedir, opendir, readdir64};
use std::marker::PhantomData;
#[derive(Debug)]
/// An iterator over directory entries from readdir64 via libc
pub struct DirIter<S>
//S is a type that implements BytesStorage, which is used to store the path bytes.
//which can take forms  Vec<u8>,Box<[u8]>,Arc<[u8]> or ideally SlimmerBytes (an alias in this crate for a smaller box type)
where
    S: BytesStorage,
{
    dir: *mut DIR,
    path_buffer: PathBuffer,
    base_len: u16,
    depth: u8,
    error: Option<Error>,
    _phantom: PhantomData<S>, //this justholds the type information for later, this compiles away due to being zero sized.
}

impl<S> DirIter<S>
where
    S: BytesStorage,
{
    #[inline]
    #[allow(dead_code)]//annoying
    pub const fn as_mut_ptr(&mut self) -> *mut u8 {
        // This function is used to get a mutable pointer to the internal buffer.
        // It is useful for operations that require direct access to the buffer.
        self.path_buffer.as_mut_ptr()
    }

    #[inline]
    #[allow(clippy::cast_lossless)]
    #[allow(clippy::cast_possible_truncation)]
    pub fn new(dir_path: &DirEntry<S>) -> Result<Self> {
        let dirp = dir_path.as_bytes();
        let dir = dirp.as_cstr_ptr(|ptr| unsafe { opendir(ptr) });
        //let dir=unsafe{opendir(cstr!(dirp))};
        //alternatively this also works if you dont like closures :)

        if dir.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
       
        let path_buffer=unsafe { init_path_buffer_readdir!(dir_path) }; //0 cost macro to construct the buffer in the way we want.

        Ok(Self {
            dir,
            path_buffer,
            base_len: dir_path.base_len,
            depth: dir_path.depth,
            error: None,
            _phantom: PhantomData, //holds storage type
        })
    }
}

impl<T> Iterator for DirIter<T>
where
    T: BytesStorage,
{
    type Item = DirEntry<T>;
    #[inline]
    #[allow(clippy::ptr_as_ptr)] //we're align so raw pointer as casts are fine.
    fn next(&mut self) -> Option<Self::Item> {
        if self.error.is_some() {
            return None;
        }

        loop {
            let entry = unsafe { readdir64(self.dir) };
            if entry.is_null() {
                return None;
            }

            let (name_file, dir_info, inode): (*const u8, u8, u64) =
                get_dirent_vals!(@minimal entry);
            // let (name_file, dir_info, inode,reclen):(*const u8,u8,u64,usize)=get_dirent_vals!(entry); //<-more efficient version to test.
            //*const u8 d8 u64, we dont need reclen, hence the @minimal tag */
            //however, reclen can be used to skip dot entries, because filtering on reclen==24 and checking dtype then pointer check.
            //we mustn't forget that this is an extremely hot loop, so avoiding as much calculation is ideal.
            skip_dot_entries!(dir_info, name_file);
            //skip_dot_entries!(dir_info, name_file, reclen);< -this is the more efficient version, but it requires reclen to be passed in.

            let full_path = unsafe { construct_path!(self, entry) };
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
impl<T> Drop for DirIter<T>
where
    T: BytesStorage,
{
    fn drop(&mut self) {
        if !self.dir.is_null() {
            unsafe { closedir(self.dir) };
        }
    }
}
