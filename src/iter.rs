


///this is currently in progress, not happy with it. it works sometimes and errors out others.
/* 
use crate::{BytesToCstrPointer, filetype::FileType, Result, direntry::{DirEntry, AlignedBuffer}};
use std::slice;
use slimmer_box::SlimmerBox;

use libc::{c_char, close, SYS_getdents64, syscall, strlen,open, O_RDONLY, PATH_MAX, dirent64};

const BUFFER_SIZE: usize = 512 * 8;

pub struct DirEntryIter {
    fd: i32,
    buffer: AlignedBuffer,
    current_offset: usize,
    bytes_remaining: usize,
    path_buffer: [u8; PATH_MAX as usize],
    path_len: usize,
    base_len: usize,
    parent_depth: u8,
}

impl Drop for DirEntryIter {
    fn drop(&mut self) {
        unsafe { close(self.fd) };
    }
}

impl Iterator for DirEntryIter {
    type Item = Result<DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current_offset >= self.bytes_remaining {
                let nread = unsafe {
                    syscall(
                        SYS_getdents64,
                        self.fd,
                        self.buffer.data.as_mut_ptr().cast::<c_char>(),
                        BUFFER_SIZE as u32,
                    )
                };
                match nread {
                    0 => return None, // End of directory
                    n if n < 0 => return Some(Err(std::io::Error::last_os_error().into())),
                    n => {
                        self.bytes_remaining = n as usize;
                        self.current_offset = 0;
                    }
                }
            }

            let entry = unsafe {
                &*(self.buffer.data.as_ptr().add(self.current_offset) as *const dirent64)
            };
            let reclen = entry.d_reclen as usize;
            self.current_offset += reclen;

            let name_ptr = entry.d_name.as_ptr();
            let name_len = unsafe { strlen(name_ptr) };
            let name_bytes = unsafe { slice::from_raw_parts(name_ptr.cast::<u8>(), name_len) };

            if name_len <= 2 && (name_bytes == b"." || name_bytes == b"..") {
                
                continue;
            }

            // Reset to base path length
            self.path_len = self.base_len;
            
            // Copy new filename to the path buffer
            if self.path_len + name_len <= self.path_buffer.len() {
                self.path_buffer[self.path_len..self.path_len + name_len].copy_from_slice(name_bytes);
                self.path_len += name_len;
            } else {
                // Path would overflow, skip this entry
                continue;
            }

            // Get current path slice
            let current_path = &self.path_buffer[..self.path_len];
            
            // Use path buffer to get file type
            let file_type = FileType::from_dtype_fallback(entry.d_type, current_path);

            // Create a new SlimmerBox from the current path
            let path = SlimmerBox::new(current_path);
            
            let entry = DirEntry {
                path,
                file_type,
                inode: entry.d_ino,
                depth: self.parent_depth + 1,
                base_len: self.base_len as u8,
            };

            return Some(Ok(entry));
        }
    }
}

impl DirEntry {
    pub fn iter(&self) -> Result<DirEntryIter> {
        let dir_path = self.as_bytes();
        let fd = dir_path.as_cstr_ptr(|ptr| unsafe { open(ptr, O_RDONLY) });
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        
        let needs_slash = dir_path != b"/";
        
        // Create path buffer and copy base path into it
        let mut path_buffer = [0u8; PATH_MAX as usize];
        let mut path_len = dir_path.len();
        
        path_buffer[..path_len].copy_from_slice(dir_path);
        
        // trailing slash if needed
        if needs_slash {
            path_buffer[path_len] = b'/';
            path_len += 1;
        }
        
        Ok(DirEntryIter {
            fd,
            buffer: AlignedBuffer { data: [0; BUFFER_SIZE] },
            current_offset: 0,
            bytes_remaining: 0,
            path_buffer,
            path_len,
            base_len: path_len,
            parent_depth: self.depth,
        })
    }
}

    */