//THIS IS A VERY SKETCHY EXPERIMENTAL OFFSHOOT, DONT PAY TOO MUCH ATTENTION.

#![cfg(target_os = "linux")]
#![allow(dead_code)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::single_char_lifetime_names)]
// THIS IS PRETTY MUCH A CARBON COPY OF `direntry.rs`
// THE ONLY DIFFERENCE IS THAT IT ALLOWS YOU TO FILTER THE ENTRIES BY A FUNCTION.
// THIS IS USEFUL IF YOU WANT TO AVOID UNNECESSARY HEAP ALLOCATIONS FOR NON-DIRECTORIES(BIG PERFORMANCE IMPACT) AND
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)] //this isnt 32bit and my division is fine.
use crate::direntry::DirEntry;
use crate::{
    AlignedBuffer, BytePath, BytesStorage, LOCAL_PATH_MAX, PathBuffer, Result, SearchConfig,
    SyscallBuffer, TempDirent, offset_dirent, traits_and_conversions::DirentConstructor,
};

/// Constructs a temporary `TempDirent<S>` from a `dirent64`/`dirent` pointer for any relevant self type
/// This is used to filter entries without allocating memory on the heap.
/// It is a temporary structure that is used to filter entries before they are converted to `DirEntry<S>`.
/// Needed to be done via macro to avoid issues with duplication/mutability of structs
///
//not used YET
macro_rules! construct_temp_dirent {
    ($self:ident, $dirent:ident) => {{
        let (d_type, inode) = unsafe {
            (
                offset_dirent!($dirent, d_type), // get d_type
                offset_dirent!($dirent, d_ino),  // get inode
            )
        };
        let base_len = $self.file_name_index();

        let full_path = $crate::utils::construct_path(&mut $self.path_buffer, base_len, $dirent);
        let file_type = $crate::FileType::from_dtype_fallback(d_type, full_path);

        $crate::TempDirent {
            path: full_path,
            file_type,
            inode,
            depth: $self.parent_depth + 1,
            file_name_index: $self.file_name_index,
            _marker: std::marker::PhantomData,
        }
    }};
}

use libc::{close, dirent64};

/// An iterator that filters directory entries based on a provided filter function.
///
/// This iterator reads directory entries using the `getdents` syscall and filters them based on
/// the provided filter function. It avoids unnecessary heap allocations by using a temporary
/// `TempDirent` struct to hold the entry data, which is then converted to a `DirEntry` when needed.
/// The iterator is designed to be efficient and to work with any type that implements the `BytesStorage` trait.
pub(crate) struct DirEntryIteratorFilter<'a, S>
where
    S: BytesStorage,
{
    pub(crate) fd: i32, //fd, this is the file descriptor of the directory we are reading from, it is used to read the directory entries via syscall
    pub(crate) buffer: SyscallBuffer, // buffer for the directory entries, this is used to read the directory entries from the file descriptor via syscall, it is 4.3k bytes~ish
    pub(crate) path_buffer: PathBuffer, // buffer for the path, this is used to construct the full path of the entry, this is reused for each entry
    pub(crate) file_name_index: u16, // base path length, this is the length of the path up to and including the last slash
    pub(crate) parent_depth: u8, // depth of the parent directory, this is used to calculate the depth of the child entries
    pub(crate) offset: usize, // offset in the buffer, this is used to keep track of where we are in the buffer
    pub(crate) remaining_bytes: i64, // remaining bytes in the buffer, this is used to keep track of how many bytes are left to read
    pub(crate) filter_func: fn(&TempDirent<S>, &SearchConfig) -> bool, // filter function, this is used to filter the entries based on the provided function
    pub(crate) search_config: &'a SearchConfig, // search configuration, this is used to pass the search configuration to the filter function
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
    pub const fn file_name_index(&self) -> usize {
        self.file_name_index as _
    }
    #[inline]
    #[allow(clippy::missing_safety_doc)]
    pub const unsafe fn next_getdents_read(&mut self) -> *const dirent64 {
        let d: *const dirent64 = unsafe { self.buffer.as_ptr().add(self.offset).cast::<_>() };
        self.offset += unsafe { offset_dirent!(d, d_reclen) }; //increment the offset by the size of the dirent structure, this is a pointer to the next entry in the buffer
        d //this is a pointer to the dirent64 structure, which contains the directory entry information
    }
    #[inline]
    /// Checks the remaining bytes in the buffer, this is a syscall that returns the number of bytes left to read.
    /// This is unsafe because it dereferences a raw pointer, so we need to ensure that
    /// the pointer is valid and that we don't read past the end of the buffer.
    pub(crate) unsafe fn getdents_syscall(&mut self) {
        self.remaining_bytes = unsafe { self.buffer.getdents64_internal(self.fd) };
        self.offset = 0;
    }

    #[inline]
    /// A function to construction a `DirEntry` from the buffer+dirent
    ///
    /// This needs unsafe because we explicitly leave implicit or explicit null pointer checks to the user (low level interface)
    pub unsafe fn construct_direntry(&mut self, drnt: *const libc::dirent64) -> DirEntry<S> {
        unsafe { self.construct_entry(drnt) }
    }

    #[inline]
    /// Prefetches the next likely entry in the buffer to keep the cache warm.
    pub(crate) fn prefetch_next_entry(&self) {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if self.offset + 128 < self.remaining_bytes as usize {
                unsafe {
                    use std::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
                    let next_entry = self.buffer.as_ptr().add(self.offset + 64).cast();
                    _mm_prefetch(next_entry, _MM_HINT_T0);
                }
            }
        }
    }

    #[inline]
    /// Prefetches the start of the buffer to keep the cache warm.
    pub(crate) fn prefetch_next_buffer(&self) {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            unsafe {
                use std::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
                _mm_prefetch(self.buffer.as_ptr().cast(), _MM_HINT_T0);
            }
        }
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
                let d: *const dirent64 = unsafe { self.next_getdents_read() }; //get next entry in the buffer,
                // this is a pointer to the dirent64 structure, which contains the directory entry information

                self.prefetch_next_entry(); //check how much is left remaining in buffer, if reasonable to hold more, warm cache
                //no op on non-x86-64

                skip_dot_or_dot_dot_entries!(d, continue); //provide the continue keyword to skip the current iteration if the entry is invalid or a dot entry

                // skip entries that are not valid or are dot entries

                let temp_dirent: TempDirent<'_, S> = construct_temp_dirent!(self, d); //construct a temporary dirent from the dirent64 pointer, this is used to filter the entries without allocating memory on the heap

                // apply the filter function to the entry
                //ive had to map the filetype to a value, it's mapped to libc dirent dtype values, this is temporary
                //while i look at implementing a decent state machine for this

                if !temp_dirent.filter(self.search_config, self.filter_func) {
                    //if the entry does not match the filter, skip it
                    continue;
                }

                return Some(temp_dirent.into()); // convert the temporary dirent to a DirEntry and return it
            }

            // prefetch the next buffer content before reading
            self.prefetch_next_buffer(); //prefetch the next buffer content to keep the cache warm, this is a no-op on non-linux systems

            // check remaining bytes
            unsafe { self.getdents_syscall() }; //get the remaining bytes in the buffer, this is a syscall that returns the number of bytes left to read

            if self.remaining_bytes <= 0 {
                // If no more entries, return None,
                return None;
            }
        }
    }
}

///////////////////////////////////////////////////////////////////////////////////////
///
/// Iterator for directory entries using getdents syscall with a filter function
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
        func: fn(&TempDirent<S>, &SearchConfig) -> bool,
    ) -> Result<impl Iterator<Item = Self>> {
        let fd = unsafe { self.open_fd()? }; //open the directory and get the file descriptor, this is used to read the directory entries via syscall

        let mut path_buffer = AlignedBuffer::<u8, { LOCAL_PATH_MAX }>::new();

        let path_len = unsafe { path_buffer.init_from_direntry(self) };

        Ok(DirEntryIteratorFilter {
            fd,
            buffer: SyscallBuffer::new(),
            path_buffer,
            file_name_index: path_len as _,
            parent_depth: self.depth,
            offset: 0,
            remaining_bytes: 0,
            filter_func: func,
            search_config,
        })
    }
}

impl<S: BytesStorage> DirentConstructor<S> for DirEntryIteratorFilter<'_, S> {
    fn path_buffer(&mut self) -> &mut PathBuffer {
        &mut self.path_buffer
    }

    fn file_index(&self) -> usize {
        self.file_name_index as usize
    }

    fn parent_depth(&self) -> u8 {
        self.parent_depth
    }
}
