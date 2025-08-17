#![allow(clippy::cast_possible_wrap)]

use crate::{
    AlignedBuffer, BytePath as _, DirEntry, DirEntryError as Error, LOCAL_PATH_MAX, PathBuffer,
    Result, custom_types_result::BytesStorage, traits_and_conversions::DirentConstructor as _,
};
use core::marker::PhantomData;
use libc::{DIR, closedir, opendir};
#[cfg(not(target_os = "linux"))]
use libc::{dirent as dirent64, readdir};
#[cfg(target_os = "linux")]
use libc::{dirent64, readdir64 as readdir}; //use readdir64 on linux

/// An iterator over directory entries from readdir (or 64 )via libc
/// General POSIX compliant directory iterator.
/// S is a type that implements `BytesStorage`, which is used to store the path bytes.
///
///
// S, which can take forms  Vec<u8>,Box<[u8]>,Arc<[u8]> or ideally SlimmerBytes (an alias in this crate for a smaller box type)
//this is only possible on linux/macos unfortunately.
pub struct DirIter<S>
where
    S: BytesStorage,
{
    pub(crate) dir: *mut DIR,
    pub(crate) path_buffer: PathBuffer,
    pub(crate) file_name_index: u16, //mainly used for indexing tricks, to trivially find the filename(avoid recalculation)
    pub(crate) parent_depth: u8, //if youve got directories bigger than 255 levels deep, you should probably rethink your life choices.
    pub(crate) error: Option<Error>,
    pub(crate) _phantom: PhantomData<S>, //this justholds the type information for later, this compiles away due to being zero sized.
}

impl<S> DirIter<S>
where
    S: BytesStorage,
{
    #[inline]
    #[allow(clippy::single_call_fn)]
    //internal function to read the directory entries
    //it is used by the new function to initialise the iterator.
    /// Returns a either
    /// Success => mutpointer to the DIR structure.
    /// Or one of many errors (permissions/etc/ ) that I haven't documented yet. They are handled explicitly however. (essentially my errortype converts from errno)
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
    /// This function reads the directory entries and populates the iterator.
    /// It is called when the iterator is created or when it needs to be reset.
    pub fn get_next_entry(&mut self) -> Option<*const dirent64> {
        let d: *const dirent64 = unsafe { readdir(self.dir) };
        //we have to check for nulls here because we're not 'buffer climbing', aka readdir has abstracted this interface.
        //we do 'buffer climb' (word i just made up) in getdents, which is why this equivalent function does not check the null in my
        //getdents iterator
        if d.is_null() {
            return None;
        }
        Some(d)
    }
    #[inline]
    /// A function to construction a `DirEntry` from the buffer+dirent
    ///
    /// This doesn't need unsafe because the pointer is already checked to not be null before it can be used here.
    pub fn construct_direntry(&mut self, drnt: *const dirent64) -> DirEntry<S> {
        unsafe { self.construct_entry(drnt) }
    }

    #[inline]
    ///now private but explanatory documentation.
    ///Constructs a new `DirIter` from a `DirEntry<S>`.
    /// This function is used to create a new iterator over directory entries.
    /// It takes a `DirEntry<S>` which contains the directory path and other metadata.
    /// It initialises the iterator by opening the directory and preparing the path buffer.
    /// Utilises libc's `opendir` and `readdir64` for directory reading.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::single_call_fn)]
    pub(crate) fn new(dir_path: &DirEntry<S>) -> Result<Self> {
        let dir = Self::open_dir(dir_path)?; //read the directory and get the pointer to the DIR structure.
        let mut path_buffer = AlignedBuffer::<u8, { LOCAL_PATH_MAX }>::new(); //this is a VERY big buffer (filepaths literally cant be longer than this)
        let base_len = unsafe { path_buffer.init_from_direntry(dir_path) };
        //mutate the buffer to contain the full path, then add a null terminator and record the new length
        //we use this length to index to get the filename (store full path -> index to get filename)
        Ok(Self {
            dir,
            path_buffer,
            file_name_index: base_len as _,
            parent_depth: dir_path.depth, //inherit depth
            error: None,                  //set noerrors
            _phantom: PhantomData,        //holds storage type
        })
    }
}

impl<S> core::fmt::Debug for DirIter<S>
where
    S: BytesStorage,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DirIter")
            .field("file_name_index", &self.file_name_index)
            .field("parent_depth", &self.parent_depth)
            .field("error", &self.error)
            .finish_non_exhaustive() //no need to expose anymore than this
    }
}

impl<T> Iterator for DirIter<T>
where
    T: BytesStorage,
{
    type Item = DirEntry<T>;
    #[inline]
    #[allow(clippy::ptr_as_ptr)] //we're aligned so raw pointer as casts are fine.
    fn next(&mut self) -> Option<Self::Item> {
        if self.error.is_some() {
            return None;
        }

        loop {
            let entry = self.get_next_entry()?; //read the next entry from the directory, this is a pointer to the dirent structure.
            //and early return if none

            skip_dot_or_dot_dot_entries!(entry, continue); //we provide the continue here to make it explicit.
            //skip . and .. entries, this macro is a bit evil, makes the code here a lot more concise

            return Some(
                self.construct_direntry(entry), //construct the dirent from the pointer, and the path buffer.
                                                //this is safe because we've already checked if it's null
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
