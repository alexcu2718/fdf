/*
//ketch of how to include extra metadata in the DirEntry struct
//but im unsure if this is the best way to do it
#![allow(dead_code)]
use crate::{DirEntry,FileType,ToStat,Result};


pub struct DirEntryMetadata{
    pub(crate)  dirent:DirEntry,
    meta:libc::stat
}

impl DirEntryMetadata {

    #[allow(clippy::missing_errors_doc)]
    pub fn new(dirent:DirEntry) -> Result<Self> {

        let stat= dirent.as_bytes().get_stat()?;

        debug_assert!(!dirent.is_unknown());

        Ok(Self {
            dirent,
            meta: stat,})





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
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.size() == 0
    }
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self.file_type(),FileType::Unknown)
    }
    #[must_use]
    pub const fn is_socket(&self) -> bool {
        matches!(self.file_type(),FileType::Socket)
    }
    #[must_use]
    pub const fn is_fifo(&self) -> bool {
        matches!(self.file_type(),FileType::Fifo)
    }
    #[must_use]
    pub const fn is_block_device(&self) -> bool {
        matches!(self.file_type(),FileType::BlockDevice)
    }
    #[must_use]
    pub const fn is_char_device(&self) -> bool {
        matches!(self.file_type(),FileType::CharDevice)
    }

    //#[must_use]
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use crate::traits_and_conversions::PathToBytes;

    #[test]
    fn test_dir_entry_metadata() {
        let test_dir = std::env::temp_dir();
        let path = std::path::PathBuf::from(test_dir).join("test_file.txt");
        std::fs::File::create(&path).unwrap();

        let dirent = DirEntry::new(path.clone()).unwrap();
        let metadata = DirEntryMetadata::new(dirent).unwrap();

        assert_eq!(metadata.size(), 0);
        assert_eq!(metadata.is_empty(), true);
        assert_eq!(metadata.is_file(), true);
        assert_eq!(metadata.is_dir(), false);
        assert_eq!(metadata.is_symlink(), false);
        assert_eq!(metadata.dirent.as_bytes(), path.to_bytes());

        assert_eq!(metadata.inode(), path.metadata().unwrap().ino());

    }
}

    */
