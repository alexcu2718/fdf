#![allow(clippy::cast_possible_wrap)]

use crate::{
    offset_ptr, BytesToCstrPointer, DirEntry, DirEntryError as Error, FileType, Result,
    LOCAL_PATH_MAX,
};
use libc::{closedir, opendir, readdir64, strlen, DIR};

#[derive(Debug)]
pub struct DirIter {
    dir: *mut DIR,
    buffer: [u8; LOCAL_PATH_MAX],
    base_len: usize,
    depth: u8,
    error: Option<Error>,
}

impl DirIter {
    #[inline]
    #[allow(clippy::cast_lossless)]
    pub fn new(dir_path: &DirEntry) -> Result<Self> {
        let dirp = dir_path.as_bytes(); //DirEntry::new(".")
        let dir = dirp.as_cstr_ptr(|ptr| unsafe { opendir(ptr) });

        if dir.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }

        let dirp_len = dirp.len();

        let needs_slash: bool = dirp != b"/";
        let base_len = dirp_len + needs_slash as usize; //casting bool to usize is trivial

        // initialise buffer with 0s; size is PATH_MAX/8, should be below 256 but on my own system theres some
        //thats 270ish, even though i cant make one, ill research another day, too lazy.
        //my terminal actually crashes when working with these files names, PUNISH THEM
        let mut buffer = [0u8; LOCAL_PATH_MAX];
        // copy directory path into buffer
        buffer[..dirp_len].copy_from_slice(dirp);

        // add trailing slash if needed
        if needs_slash {
            buffer[dirp_len] = b'/';
        }

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
            //dbg!(&self.error);
            return None;
        }

        loop {
            let entry = unsafe { readdir64(self.dir) };
            if entry.is_null() {
                // end of directory stream or error occurred

                return None;
            }

            let name_file: *const i8 = unsafe { offset_ptr!(entry, d_name).cast() };

            //skip . and .., these will cause recursion ofc but also theyre expressed as i8 arrays
            //that are either [46, 46, 0] or [46, 0, 0] therefore we can just check the first 3 bytes
            //thus avoiding strlen on all these.
            unsafe {
                if *name_file.add(0) == 46 {
                    //test the easiest condition first
                    //short circuit with this one as its easier
                    if *name_file.add(1) == 0 || (*name_file.add(1) == 46 && *name_file.add(2) == 0)
                    {
                        continue;
                    }
                }
            }

            let name_len = unsafe { strlen(name_file) };

            let name_bytes: &[u8] =
                unsafe { std::slice::from_raw_parts(name_file.cast(), name_len) };
            //THIS CANNOT BE A CONST POINTER CAST, IT WILL BREAK DUE TO SOME SHIT THAT TOOK A WHILE TO UNDERSTAND.

            // calculate totak buffer capacity
            let total_path_len = self.base_len + name_len;

            // copy filename into buffer
            self.buffer[self.base_len..total_path_len].copy_from_slice(name_bytes);

            // get valid path slice
            let full_path = &self.buffer[..total_path_len];

            // get file type
            let dir_info = unsafe { *offset_ptr!(entry, d_type).cast() };

            let file_type = FileType::from_dtype_fallback(dir_info, full_path);

            debug_assert!(file_type == FileType::from_dtype(dir_info));

            #[allow(clippy::cast_possible_truncation)] // this numbers involved never exceed u8
            // return the directory entry
            return Some(DirEntry {
                path: full_path.into(),
                file_type,
                inode: unsafe { *offset_ptr!(entry, d_ino) },
                depth: self.depth + 1,
                base_len: self.base_len as u8,
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
