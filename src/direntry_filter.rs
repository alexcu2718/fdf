#![cfg(target_os = "linux")]
#![allow(clippy::single_char_lifetime_names)]
// THIS IS PRETTY MUCH A CARBON COPY OF `direntry.rs`
// THE ONLY DIFFERENCE IS THAT IT ALLOWS YOU TO FILTER THE ENTRIES BY A FUNCTION.
// THIS IS USEFUL IF YOU WANT TO AVOID UNNECESSARY ALLOCATIONS AND
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)] //this isnt 32bit and my division is fine.
use crate::direntry::DirEntry;
use crate::{
    BytePath, BytesStorage, FileType, PathBuffer, Result, SearchConfig, SyscallBuffer,
    construct_path, init_path_buffer, offset_ptr, skip_dot_or_dot_dot_entries,
};
#[cfg(target_arch = "x86_64")]
use crate::{prefetch_next_buffer, prefetch_next_entry};
use libc::{O_CLOEXEC, O_DIRECTORY, O_NONBLOCK, O_RDONLY, X_OK, access, close, dirent64, open};
use std::marker::PhantomData;

pub struct DirEntryIteratorFilter<'a, S>
where
    S: BytesStorage,
{
    pub(crate) fd: i32, //fd, this is the file descriptor of the directory we are reading from, it is used to read the directory entries via syscall
    pub(crate) buffer: SyscallBuffer, // buffer for the directory entries, this is used to read the directory entries from the file descriptor via syscall, it is 4.3k bytes~ish
    pub(crate) path_buffer: PathBuffer, // buffer for the path, this is used to construct the full path of the entry, this is reused for each entry
    pub(crate) base_len: u16, // base path length, this is the length of the path up to and including the last slash
    pub(crate) parent_depth: u8, // depth of the parent directory, this is used to calculate the depth of the child entries
    pub(crate) offset: usize, // offset in the buffer, this is used to keep track of where we are in the buffer
    pub(crate) remaining_bytes: i64, // remaining bytes in the buffer, this is used to keep track of how many bytes are left to read
    pub(crate) filter_func: fn(&TempDirent, &SearchConfig) -> bool, // filter function, this is used to filter the entries based on the provided function
    pub(crate) search_config: &'a SearchConfig, // search configuration, this is used to pass the search configuration to the filter function
    _marker: PhantomData<S>, //placeholder for the storage type, this is used to ensure that the iterator can be used with any storage type
}

impl<S> Drop for DirEntryIteratorFilter<'_, S>
where
    S: BytesStorage,
{
    /// Drops the iterator, closing the file descriptor.
    /// same as above, we need to close the file descriptor when the iterator is dropped to avoid resource leaks.
    #[inline]
    fn drop(&mut self) {
        unsafe { close(self.fd) };
    }
}

impl<S> DirEntryIteratorFilter<'_, S>
where
    S: BytesStorage,
{
    /// Returns the base length of the path buffer.
    #[inline]
    pub const fn base_len(&self) -> usize {
        self.base_len as _
    }
}

pub struct TempDirent<'a> {
    pub(crate) path: &'a [u8], // path of the entry, this is used to store the path of the entry 16b (64bit)
    pub(crate) depth: u8, // depth of the entry, this is used to calculate the depth of   1bytes
    pub(crate) file_type: FileType, // file type of the entry, this is used to determine the type of the entry 1bytes
    pub(crate) base_len: u16,       //used to calculate filename via indexing 2bytes
    pub(crate) inode: u64, // inode of the entry, this is used to uniquely identify the entry 8bytes
}

impl std::ops::Deref for TempDirent<'_> {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.path
    }
}

/// A temporary directory entry used for filtering purposes.
/// This struct is used to store the path, depth, file type and base length of the
/// entry, so we can filter entries without allocating memory on the heap.
/// It is used in the `DirEntryIteratorFilter` iterator to filter entries based on the
/// provided filter function.
impl<'a> TempDirent<'a> {
    /// Returns a new `TempDirent` with the given path, depth, file type and base length.
    /// for filtering purposes (so we can avoid heap allocations)
    #[inline]
    pub const fn new(path: &'a [u8], depth: u8, file_type: FileType, base_len: u16,ino:u64) -> Self {
        Self {
            path,
            depth,
            file_type,
            base_len,
            inode: ino, //inode is used to uniquely identify the entry, this is used to avoid duplicates
        }
    }

    /// Converts the temporary dirent into a `DirEntry`.
    #[inline]
    pub fn to_direntry<S>(self) -> DirEntry<S>
    where
        S: BytesStorage,
    {
        // Converts the temporary dirent into a DirEntry, this is used to convert the temporary dirent into a DirEntry
        DirEntry {
            path: self.path.into(),
            file_type: self.file_type,
            inode: self.inode,
            depth: self.depth,
            base_len: self.base_len,




        }
    }

    #[inline]
    pub fn matches_extension(&self, ext: &[u8]) -> bool {
        // Checks if the file name ends with the given extension
        // this is used to filter entries by extension
        self.file_name().matches_extension(ext)
    }
    #[inline]
    pub const fn inode(&self) -> u64 {
        // Returns the inode of the entry, this is used to uniquely identify the entry
        self.inode
    }

    #[inline]
    pub const fn depth(&self) -> usize {
        self.depth as _
    }
    #[inline]
    pub fn realpath(&self) -> crate::Result<&[u8]> {
        self.path.realpath()
    }

    #[inline]
    pub fn matches_path(&self, file_name_only: bool, cfg: &SearchConfig) -> bool {
        // Checks if the entry matches the search configuration
        // this is used to filter entries by path
        cfg.matches_path_internal(self.path, file_name_only, self.base_len as usize)
    }

    #[inline]
    #[must_use]
    pub const fn is_traversible(&self) -> bool {
        //this is a cost free check, we just check if the file type is a directory or symlink
        self.file_type.is_traversible()
    }
    #[inline]
    pub const fn path(&self) -> &[u8] {
        // Returns the path of the entry, this is used to get the path of the entry
        self.path
    }

    #[inline]
    #[must_use]
    ///costly check for executables
    pub fn is_executable(&self) -> bool {
        //X_OK is the execute permission, requires access call
        self.is_regular_file() && unsafe { self.path.as_cstr_ptr(|ptr| access(ptr, X_OK) == 0) }
    }

    #[inline]
    #[must_use]
    pub fn is_readable(&self) -> bool {
        //R_OK is the read permission, requires access call
        self.path.is_readable()
    }
    #[inline]
    #[must_use]
    pub fn is_writable(&self) -> bool {
        //W_OK is the write permission,
        self.path.is_writable()
    }

    #[inline]
    pub const fn file_name_index(&self) -> usize {
        // Returns the index of the file name in the path, this is used to get the file name of the entry
        // it is calculated by adding the base length to the depth of the entry
        self.base_len as usize
    }
    #[inline]
    pub fn file_name(&self) -> &[u8] {
        // Returns the file name of the entry, this is used to get the file name of the entry
        // it is calculated by slicing the path from the base length to the end of the path
        unsafe { self.path.get_unchecked(self.file_name_index()..) }
    }
    ///cost free check for block devices
    #[inline]
    #[must_use]
    pub const fn is_block_device(&self) -> bool {
        self.file_type.is_block_device()
    }

    ///Cost free check for character devices
    #[inline]
    #[must_use]
    pub const fn is_char_device(&self) -> bool {
        self.file_type.is_char_device()
    }

    ///Cost free check for fifos
    #[inline]
    #[must_use]
    pub const fn is_fifo(&self) -> bool {
        self.file_type.is_fifo()
    }

    ///Cost free check for sockets
    #[inline]
    #[must_use]
    pub const fn is_socket(&self) -> bool {
        self.file_type.is_socket()
    }

    ///Cost free check for regular files
    #[inline]
    #[must_use]
    pub const fn is_regular_file(&self) -> bool {
        self.file_type.is_regular_file()
    }

    ///Cost free check for directories
    #[inline]
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        self.file_type.is_dir()
    }
    ///cost free check for unknown file types
    #[inline]
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        self.file_type.is_unknown()
    }
    ///cost free check for symlinks
    #[inline]
    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        self.file_type.is_symlink()
    }

    #[inline]
    pub fn filter(&self, cfg: &SearchConfig, func: fn(&Self, &SearchConfig) -> bool) -> bool {
        func(self, cfg)
    }
}

impl<S> Iterator for DirEntryIteratorFilter<'_, S>
where
    S: BytesStorage,
{
    type Item
        = DirEntry<S>
    where
        S: BytesStorage;
    #[inline]
    #[allow(clippy::cast_lossless)] //casting a u16 to u64 is lossless as i am doing but you can cast to your own type, enjoy;
    #[allow(clippy::ptr_as_ptr)] //aligned pointers are what we're using.
    /// Returns the next directory entry in the iterator.
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If we have remaining data in buffer, process it
            if self.offset < self.remaining_bytes as usize {
                let d: *const dirent64 = unsafe { self.buffer.next_getdents_read(self.offset) }; //get next entry in the buffer,
                // this is a pointer to the dirent64 structure, which contains the directory entry information

                #[cfg(target_arch = "x86_64")]
                prefetch_next_entry!(self); //check how much is left remaining in buffer, if reasonable to hold more, warm cache
                self.offset += unsafe { offset_ptr!(d, d_reclen) }; //index to next entry, so when we call next again, we will get the next entry in the buffer

                // skip entries that are not valid or are dot entries
                skip_dot_or_dot_dot_entries!(d, continue); //provide the continue keyword to skip the current iteration if the entry is invalid or a dot entry
                let (d_type, inode) = unsafe {
                    (
                        *offset_ptr!(d, d_type), //get the d_type from the dirent structure, this is the type of the entry
                        offset_ptr!(d, d_ino),   //get the inode
                    )
                };

                // skip entries that are not valid or are dot entries
                let full_path = unsafe { construct_path!(self, d) }; //a macro that constructs it, the full details are a bit lengthy
                //but essentially its null initialised buffer, copy the starting path (+an additional slash if needed) and copy name of entry
                //this is probably the cheapest way to do it, as it avoids unnecessary allocations and copies.

                let depth = self.parent_depth + 1; // increment depth for child entries

                let file_type = FileType::from_dtype_fallback(d_type, full_path); //if d_type is unknown fallback to lstat otherwise we get for freeeeeeeee

                let temp_dirent = TempDirent::new(full_path, depth, file_type, self.base_len,inode); //create a temporary dirent, this is used to store the path, depth and file type of the entry

                // apply the filter function to the entry
                //ive had to map the filetype to a value, it's mapped to libc dirent dtype values, this is temporary
                //while i look at implementing a decent state machine for this
                if !temp_dirent.filter(self.search_config, self.filter_func) {
                    //if the entry does not match the filter, skip it
                    continue;
                }
         

             

                return Some(temp_dirent.to_direntry()); // convert the temporary dirent to a DirEntry and return it
            }

            // prefetch the next buffer content before reading
            #[cfg(target_arch = "x86_64")]
            prefetch_next_buffer!(self);

            // check remaining bytes
            self.remaining_bytes = unsafe { self.buffer.getdents64(self.fd) };
            self.offset = 0;

            if self.remaining_bytes <= 0 {
                // If no more entries, return None,
                return None;
            }
        }
    }
}

///////////////////////////////////////////////////////////////////////////////////////
/// // Iterator for directory entries using getdents syscall with a filter function
#[allow(clippy::multiple_inherent_impl)] // this is a separate impl block to avoid confusion with the other iterator
impl<S> DirEntry<S>
where
    S: BytesStorage,
{
    #[inline]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    #[allow(clippy::cast_possible_wrap)]
    ///`getdents_filter` is an iterator over fd,where each consequent index is a directory entry.
    /// This function is a low-level syscall wrapper that reads directory entries.
    /// It returns an iterator that yields `DirEntry` objects.
    /// This differs from my `as_iter` impl, which uses libc's `readdir64`, this uses `libc::syscall(SYS_getdents64.....)`
    ///this differs from `getdents` in that it allows you to filter the entries by a function.
    /// so it avoids a lot of unnecessary allocations and copies :)
    pub fn getdents_filter(
        &self,
        search_config: &SearchConfig,
        func: fn(&TempDirent, &SearchConfig) -> bool,
    ) -> Result<impl Iterator<Item = Self>> {
        let fd = self
            .as_cstr_ptr(|ptr| unsafe { open(ptr, O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) });
        //alternatively syntaxes I made.
        //let fd= unsafe{ open(cstr_n!(dir_path,256),O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) };
        //let fd= unsafe{ open(cstr!(dir_path),O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) };
        // let fd=unsafe{open_asm(dir_path)};

        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        let (path_len, path_buffer) = unsafe { init_path_buffer!(self) }; // (we provide the depth for some quick checks)

        Ok(DirEntryIteratorFilter {
            fd,
            buffer: SyscallBuffer::new(),
            path_buffer,
            base_len: path_len as _,
            parent_depth: self.depth,
            offset: 0,
            remaining_bytes: 0,
            filter_func: func,
            search_config,
            _marker: PhantomData::<S>, // marker for the storage type, this is used to ensure that the iterator can be used with any storage type
        })
    }
}
