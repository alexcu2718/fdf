#![allow(clippy::cast_possible_wrap)]
#[allow(unused_imports)]
use crate::{
    BytePath, DirEntry, DirEntryError as Error, FileType, PathBuffer, Result, SyscallBuffer,
    construct_path, cstr, custom_types_result::BytesStorage, init_path_buffer, offset_ptr,
    skip_dot_or_dot_dot_entries,
};
#[cfg(not(target_os = "linux"))]
use libc::readdir;
#[cfg(target_os = "linux")]
use libc::readdir64 as readdir; //use readdir64 on linux
use libc::{DIR, closedir, opendir};
use std::marker::PhantomData; //use readdir on other platforms, this is the standard POSIX function
#[derive(Debug)]
/// An iterator over directory entries from readdir (or 64 )via libc
/// General POSIX compliant directory iterator.
pub struct DirIter<S>
//S is a type that implements BytesStorage, which is used to store the path bytes.
//which can take forms  Vec<u8>,Box<[u8]>,Arc<[u8]> or ideally SlimmerBytes (an alias in this crate for a smaller box type)
//this is only possible on linux unfortunately.
where
    S: BytesStorage,
{
    dir: *mut DIR,
    path_buffer: PathBuffer,
    base_len: u16, //mainly used for indexing tricks, to trivially find the filename(avoid recalculation)
    depth: u8, //if youve got directories bigger than 255 levels deep, you should probably rethink your life choices.
    error: Option<Error>,
    _phantom: PhantomData<S>, //this justholds the type information for later, this compiles away due to being zero sized.
}

impl<S> DirIter<S>
where
    S: BytesStorage,
{
    #[inline]
    #[allow(dead_code)] //annoying
    pub const fn as_mut_ptr(&mut self) -> *mut u8 {
        // This function is used to get a mutable pointer to the internal buffer.
        // It is useful for operations that require direct access to the buffer.
        self.path_buffer.as_mut_ptr()
    }

    pub const fn base_len(&self) -> usize {
        // This function returns the base length of the path buffer.
        // It is used to determine the length of the base path for constructing full paths.
        self.base_len as usize
    }

    #[inline]
    #[allow(clippy::cast_lossless)]
    #[allow(clippy::cast_possible_truncation)]
    ///Constructs a new `DirIter` from a `DirEntry<S>`.
    /// This function is used to create a new iterator over directory entries.
    /// It takes a `DirEntry<S>` which contains the directory path and other metadata.
    /// It initialises the iterator by opening the directory and preparing the path buffer.
    /// Utilises libc's `opendir` and `readdir64` for directory reading.
    /// # Errors
    /// TBD
    pub fn new(dir_path: &DirEntry<S>) -> Result<Self> {
        let dir = dir_path.as_cstr_ptr(|ptr| unsafe { opendir(ptr) });
        //let dir=unsafe{opendir(cstr!(dir_path))};
        //alternatively this also works if you dont like closures :)

        if dir.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }

        let (base_len, path_buffer) = unsafe { init_path_buffer!(dir_path) }; //0 cost macro to construct the buffer in the way we want.

        Ok(Self {
            dir,
            path_buffer,
            base_len: base_len as _,
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
            let entry = unsafe { readdir(self.dir) };

            if entry.is_null() {
                return None;
            }

            skip_dot_or_dot_dot_entries!(entry, continue); //we provide the continue here to make it explicit.
            //skip . and .. entries, this macro is a bit evil, makes the code here a lot more concise

            let (d_type, inode) = unsafe {
                (
                    *offset_ptr!(entry, d_type), //get the d_type from the dirent structure, this is the type of the entry
                    offset_ptr!(entry, d_ino),
                ) //get the inode
            };

            let full_path = unsafe { construct_path!(self, entry) };
            return Some(DirEntry {
                path: full_path.into(),
                file_type: FileType::from_dtype_fallback(d_type, full_path), //most of the time we get filetype from the value but not always, uses lstat if needed
                inode,
                depth: self.depth + 1,   //increment depth for each entry
                base_len: self.base_len, //inherit base_len from the parent directory
            });
        }
    }
}
impl<T> Drop for DirIter<T>
where
    T: BytesStorage,
{
    #[inline]
    fn drop(&mut self) {
        if !self.dir.is_null() {
            unsafe { closedir(self.dir) };
        }
    }
}
