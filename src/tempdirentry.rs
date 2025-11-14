
use core::ffi::CStr;
use crate::FileType;
use core::cell::Cell;
use crate::DirEntry;
pub struct TempDirEntry<'nextcall> {
 
    pub(crate) path: &'nextcall CStr, //16 bytes

    pub(crate) file_type: FileType,

    pub(crate) inode: u64, //8 bytes
  
    pub(crate) depth: u32, //4bytes

    pub(crate) file_name_index: usize, //8 bytes

    pub(crate) is_traversible_cache: Cell<Option<bool>>, //1byte
} //38 bytes, rounded to 40


impl TempDirEntry<'_>{


    pub fn to_owned(self)->DirEntry{
        DirEntry { path: self.path.into(), file_type: self.file_type, 
            inode: self.inode, depth: self.depth, file_name_index: self.file_name_index, is_traversible_cache: self.is_traversible_cache }
    }
}