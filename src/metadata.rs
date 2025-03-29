
//sketch of how to include extra metadata in the DirEntry struct
//but im unsure if this is the best way to do it
/* 
use crate::{DirEntry,DirEntryError,FileType,get_stat_bytes};


pub struct DirEntryMetadata{
    pub(crate)  dirent:DirEntry,
    meta:libc::stat
}

impl DirEntryMetadata {
    #[allow(clippy::missing_errors_doc)]
    pub fn new(dirent:DirEntry) -> Result<Self,DirEntryError> {
        let stat=get_stat_bytes(&dirent.path)?;
        
        Ok(Self {
            dirent,
            meta: stat})
            
        
    }
    #[must_use]
    pub const fn inode(&self) -> u64 {
        self.meta.st_ino
    }
    #[must_use]
    pub const fn as_direntry(&self) -> &DirEntry {
        &self.dirent
    }
    #[must_use]
    pub const  fn size(&self) -> i64 {
        self.meta.st_size 
    }
    #[must_use]
    pub const  fn accessed(&self) -> i64 {
        self.meta.st_atime
    }
    #[must_use]
    pub const fn created(&self) -> i64 {
        self.meta.st_ctime
    }
    #[must_use]
    pub const fn modified(&self) -> i64 {
        self.meta.st_mtime
    }
    #[must_use]
    pub const fn permissions(&self) -> u32 {
        self.meta.st_mode
    }
   
    #[must_use]
    pub const  fn file_type(&self) -> FileType {
        self.dirent.file_type()
    }
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        self.dirent.is_dir()
    }
    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        self.dirent.is_symlink()
    }

    #[must_use]
    pub const fn is_file(&self) -> bool {
        self.dirent.is_regular_file()
    }
}

    */