#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)] //this isnt 32bit and my division is fine.
use crate::direntry::DirEntry;
use crate::{
    BytePath, BytesStorage, FileType, PathBuffer, Result, SyscallBuffer, construct_path,
    get_dirent_vals, init_path_buffer_syscall, prefetch_next_buffer, prefetch_next_entry,
    skip_dot_entries,
};
use libc::{O_CLOEXEC, O_DIRECTORY, O_NONBLOCK, O_RDONLY, close, dirent64, open};
use std::marker::PhantomData;

pub struct DirEntryIteratorFilter<S>
where
    S: BytesStorage,
{
    pub(crate) fd: i32, //fd, this is the file descriptor of the directory we are reading from, it is used to read the directory entries via syscall
    pub(crate) buffer: SyscallBuffer, // buffer for the directory entries, this is used to read the directory entries from the file descriptor via syscall, it is 4.3k bytes~ish
    pub(crate) path_buffer: PathBuffer, // buffer for the path, this is used to construct the full path of the entry, this is reused for each entry
    pub(crate) base_path_len: u16, // base path length, this is the length of the path up to and including the last slash
    pub(crate) parent_depth: u8, // depth of the parent directory, this is used to calculate the depth of the child entries
    pub(crate) offset: usize, // offset in the buffer, this is used to keep track of where we are in the buffer
    pub(crate) remaining_bytes: i64, // remaining bytes in the buffer, this is used to keep track of how many bytes are left to read
    pub(crate) filter_func: fn(&[u8], usize, u8) -> bool, // filter function, this is used to filter the entries based on the provided function
    _marker: PhantomData<S>, //mainly the arguments would be full path,depth,filetype, this is a shoddy implementation but im testing waters.
}

impl<S> Drop for DirEntryIteratorFilter<S>
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

impl<S> Iterator for DirEntryIteratorFilter<S>
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
                prefetch_next_entry!(self);

                // Extract the fields from the dirent structure

                let (name_ptr, d_type, reclen, inode): (*const u8, u8, usize, u64) =
                    get_dirent_vals!(d);

                self.offset += reclen; //index to next entry, so when we call next again, we will get the next entry in the buffer

                // skip entries that are not valid or are dot entries
                skip_dot_entries!(d_type, name_ptr); //requiring d_type is just a niche optimisation, it allows us not to do 'as many' pointer checks
                let full_path = unsafe { construct_path!(self, name_ptr) }; //a macro that constructs it, the full details are a bit lengthy
                //but essentially its null initialised buffer, copy the starting path (+an additional slash if needed) and copy name of entry
                //this is probably the cheapest way to do it, as it avoids unnecessary allocations and copies.

                let depth = self.parent_depth + 1; // increment depth for child entries

                let file_type = FileType::from_dtype_fallback(d_type, full_path); //if d_type is unknown fallback to lstat otherwise we get for freeeeeeeee

                // apply the filter function to the entry
                //ive had to map the filetype to a value, it's mapped to libc dirent dtype values, this is temporary
                //while i look at implementing a decent state machine for this
                if !(self.filter_func)(full_path, depth as usize, file_type.d_type_value()) {
                    //if the entry does not match the filter, skip it
                    continue;
                }

                let entry = DirEntry {
                    path: full_path.into(),
                    file_type, //if d_type is unknown fallback to lstat otherwise we get for freeeeeeeee
                    inode,
                    depth,
                    base_len: self.base_path_len,
                };

                return Some(entry);
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
        func: fn(&[u8], usize, u8) -> bool,
    ) -> Result<impl Iterator<Item = Self>> {
        let dir_path = self.as_bytes();
        let fd = dir_path
            .as_cstr_ptr(|ptr| unsafe { open(ptr, O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) });
        //alternatively syntaxes I made.
        //let fd= unsafe{ open(cstr_n!(dir_path,256),O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) };
        //let fd= unsafe{ open(cstr!(dir_path),O_RDONLY, O_NONBLOCK, O_DIRECTORY, O_CLOEXEC) };
        // let fd=unsafe{open_asm(dir_path)};

        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        let mut path_buffer = PathBuffer::new(); // buffer for the path, this is used(the pointer is mutated) to construct the full path of the entry, this is actually
        //a uninitialised buffer, which is then initialised with the directory path
        let mut path_len = dir_path.len();
        init_path_buffer_syscall!(path_buffer, path_len, dir_path, self); // initialise the path buffer with the directory path

        Ok(DirEntryIteratorFilter {
            fd,
            buffer: SyscallBuffer::new(),
            path_buffer,
            base_path_len: path_len as _,
            parent_depth: self.depth,
            offset: 0,
            remaining_bytes: 0,
            filter_func: func,
            _marker: PhantomData::<S>, // marker for the storage type, this is used to ensure that the iterator can be used with any storage type
        })
    }
}
