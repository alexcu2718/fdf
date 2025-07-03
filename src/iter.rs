#![allow(clippy::cast_possible_wrap)]
#[allow(unused_imports)]
use crate::{
    BytePath, DirEntry, DirEntryError as Error, FileType, PathBuffer, Result, SyscallBuffer, cstr,
    cursed_macros::construct_dirent, cursed_macros::construct_path,
    cursed_macros::init_path_buffer, cursed_macros::skip_dot_or_dot_dot_entries,
    custom_types_result::BytesStorage, offset_ptr,
};
use libc::{DIR, closedir, opendir};
#[cfg(not(target_os = "linux"))]
use libc::{dirent, readdir};
#[cfg(target_os = "linux")]
use libc::{dirent64 as dirent, readdir64 as readdir}; //use readdir64 on linux
use std::marker::PhantomData; //use readdir on other platforms, this is the standard POSIX function

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
    file_name_index: u16, //mainly used for indexing tricks, to trivially find the filename(avoid recalculation)
    parent_depth: u8, //if youve got directories bigger than 255 levels deep, you should probably rethink your life choices.
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
    #[inline]
    pub const fn file_name_index(&self) -> usize {
        // This function returns the base length of the path buffer.
        // It is used to determine the length of the base path for constructing full paths.
        self.file_name_index as _
    }
    #[inline]
    //internal function to read the directory entries
    //this is a private function that reads the directory entries and returns a pointer to the DIR
    //it is used by the new function to initialise the iterator.
    /// Reads the directory entries and returns a pointer to the DIR structure.
    pub(crate) fn open_dir(direntry: &DirEntry<S>) -> Result<*mut DIR> {
        let dir = direntry.as_cstr_ptr(|ptr| unsafe { opendir(ptr) });
        // This function reads the directory entries and populates the iterator.
        // It is called when the iterator is created or when it needs to be reset.
        if dir.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }

        Ok(dir)
    }
    #[inline]
    /// Reads the next directory entry from the iterator.
    pub fn read_dir(&mut self) -> Option<*const dirent> {
        // This function reads the directory entries and populates the iterator.
        // It is called when the iterator is created or when it needs to be reset.
        let d: *const dirent = unsafe { readdir(self.dir) };
        // This function reads the directory entries and returns a pointer to the dirent structure.
        // It is used by the next function to get the next entry in the directory.
        if d.is_null() {
            return None;
        }
        Some(d)
    }
    #[inline]
    #[allow(dead_code)] //annoying
    /// Reads the next directory entry without checking for null pointers.
    /// This function is unsafe because it does not check if the directory pointer is null.
    /// SAFETY: This function assumes that the directory pointer is valid and not null.
    pub unsafe fn read_dir_unchecked(&mut self) -> *const dirent {
        // This function reads the directory entries without checking for null pointers.
        // It is used internally by the iterator to read the next entry.
        unsafe { readdir(self.dir) }
    }

    #[inline]
    #[allow(clippy::cast_lossless)]
    #[allow(clippy::cast_possible_truncation)]
    ///Constructs a new `DirIter` from a `DirEntry<S>`.
    /// This function is used to create a new iterator over directory entries.
    /// It takes a `DirEntry<S>` which contains the directory path and other metadata.
    /// It initialises the iterator by opening the directory and preparing the path buffer.
    /// Utilises libc's `opendir` and `readdir64` for directory reading.
    pub(crate) fn new(dir_path: &DirEntry<S>) -> Result<Self> {
        let dir = Self::open_dir(dir_path)?; //read the directory and get the pointer to the DIR structure.
        let (base_len, path_buffer) = unsafe { init_path_buffer!(dir_path) }; //0 cost macro to construct the buffer in the way we want.

        Ok(Self {
            dir,
            path_buffer,
            file_name_index: base_len as _,
            parent_depth: dir_path.depth,
            error: None,
            _phantom: PhantomData, //holds storage type
        })
    }
}

impl<S> std::fmt::Debug for DirIter<S>
where
    S: BytesStorage,
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirIter")
            .field("path_buffer", &self.path_buffer)
            .field("file_name_index", &self.file_name_index)
            .field("parent_depth", &self.parent_depth)
            .field("error", &self.error)
            .finish_non_exhaustive() //we're not including dir pointer here, as it is not safe to expose(and its fairly useless to the user)
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
            let entry = self.read_dir()?; //read the next entry from the directory, this is a pointer to the dirent structure.

            skip_dot_or_dot_dot_entries!(entry, continue); //we provide the continue here to make it explicit.
            //skip . and .. entries, this macro is a bit evil, makes the code here a lot more concise

            return Some(
                construct_dirent!(self, entry), //construct the dirent from the pointer, and the path buffer.
            );
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
