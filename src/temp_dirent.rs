#![allow(clippy::single_char_lifetime_names)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(clippy::missing_errors_doc)]
#![cfg(target_os="linux")]
use crate::BytesStorage;
use crate::DirEntry;
use crate::FileType;
use crate::SearchConfig;
use crate::access_dirent;
use crate::traits_and_conversions::BytePath as _;
use core::marker::PhantomData;
#[cfg(not(target_os = "linux"))]
use libc::dirent as dirent64;
#[cfg(target_os = "linux")]
use libc::dirent64;
/// A temporary directory entry used for filtering purposes.
/// Used to avoid heap allocations.
///
/// This struct is used to store the path, depth, file type and base length of the
/// entry, so we can filter entries without allocating memory on the heap.
/// It is used in the `DirEntryIteratorFilter` iterator to filter entries based on the
/// provided filter function.
pub struct TempDirent<'a, S> {
    pub(crate) path: &'a [u8], // path of the entry, this is used to store the path of the entry 16b (64bit)
    pub(crate) depth: u8, // depth of the entry, this is used to calculate the depth of   1bytes
    pub(crate) file_type: FileType, // file type of the entry, this is used to determine the type of the entry 1bytes
    pub(crate) file_name_index: u16, //used to calculate filename via indexing 2bytes
    pub(crate) inode: u64, // inode of the entry, this is used to uniquely identify the entry 8bytes
    pub(crate) _marker: PhantomData<S>, // placeholder for the storage type, this is used to ensure that the temporary dirent can be used with any storage type
}

impl<S> core::ops::Deref for TempDirent<'_, S> {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.path
    }
}

impl<S> core::convert::AsRef<[u8]> for TempDirent<'_, S> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.path
    }
}

impl<S> From<TempDirent<'_, S>> for DirEntry<S>
where
    S: BytesStorage,
{
    #[inline]
    fn from(val: TempDirent<'_, S>) -> Self {
        val.to_direntry()
    }
}

impl<S> core::fmt::Debug for TempDirent<'_, S>
where
    S: BytesStorage,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TempDirent")
            .field("path", &self.path.to_string_lossy())
            .field("file_name", &self.file_name())
            .field("depth", &self.depth)
            .field("file_type", &self.file_type)
            .field("base_len", &self.file_name_index)
            .field("inode", &self.inode)
            .finish()
    }
}

/// A temporary directory entry used for filtering purposes.
/// This struct is used to store the path, depth, file type and base length of the
/// entry, so we can filter entries without allocating memory on the heap.
/// It is used in the `DirEntryIteratorFilter` iterator to filter entries based on the
/// provided filter function.
impl<'a, S> TempDirent<'a, S>
where
    S: BytesStorage,
{
    /// Returns a new `TempDirent` with the given path, depth, file type and base length.
    /// for filtering purposes (so we can avoid heap allocations)
    #[inline]
    pub fn new(path: &'a [u8], depth: u8, base_len: u16, dirent: *const dirent64) -> Self {
        let (d_type, inode) = unsafe {
            (
                access_dirent!(dirent, d_type), //get the d_type from the dirent structure, this is the type of the entry
                access_dirent!(dirent, d_ino),  //get the inode
            )
        };

        Self {
            path,
            depth,
            file_type: FileType::from_dtype_fallback(d_type, path), //file type is mapped to a FileType enum, this is used to determine the type of the entry
            file_name_index: base_len,
            inode, //inode is used to uniquely identify the entry, this is used to avoid duplicates
            _marker: PhantomData::<S>, // marker for the storage type, this is used to ensure that the temporary dirent can be used with any storage type
        }
    }

    /// Converts the temporary dirent into a `DirEntry`.
    #[inline]
    pub fn to_direntry(&self) -> DirEntry<S> {
        // Converts the temporary dirent into a DirEntry, this is used to convert the temporary dirent into a DirEntry
        DirEntry {
            path: self.path.into(),
            file_type: self.file_type,
            inode: self.inode,
            depth: self.depth,
            file_name_index: self.file_name_index,
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
    pub fn matches_path(&self, file_name_only: bool, cfg: &SearchConfig) -> bool {
        // Checks if the entry matches the search configuration
        // this is used to filter entries by path
        cfg.matches_path_internal(self.path, file_name_only, self.file_name_index())
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
        self.is_regular_file()
            && unsafe {
                self.path
                    .as_cstr_ptr(|ptr| libc::access(ptr, libc::X_OK) == 0i32)
            }
    }

    #[inline]
    #[must_use]
    ///Checks if path is readable
    pub fn is_readable(&self) -> bool {
        //R_OK is the read permission, requires access call
        self.path.is_readable()
    }
    #[inline]
    #[must_use]
    pub fn is_writable(&self) -> bool {
        self.path.is_writable()
    }

    #[inline]
    #[must_use]
    #[allow(clippy::wildcard_enum_match_arm)]
    ///costly check for empty files'
    /// returns false for errors/char devices/sockets/fifos/etc, mostly useful for files and directories
    /// for files, it checks if the size is zero without loading all metadata
    /// for directories, it checks if they have no entries
    /// for special files like devices, sockets, etc., it returns false
    pub fn is_empty(&self) -> bool
    where
        S: BytesStorage,
    {
        match self.file_type {
            FileType::RegularFile => {
                self.size().is_ok_and(|size| size == 0u64)
                //this checks if the file size is zero, this is a costly check as it requires a stat call
            }
            FileType::Directory => {
                self.to_direntry()
                    .readdir() //if we can read the directory, we check if it has no entries
                    .is_ok_and(|mut entries| entries.next().is_none())
            }
            _ => false,
        }
    }

    #[inline]
    pub const fn file_name_index(&self) -> usize {
        // Returns the index of the file name in the path, this is used to get the file name of the entry
        // it is calculated by adding the base length to the depth of the entry
        self.file_name_index as _
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

    ///Cost free check for pipes (FIFOs)
    #[inline]
    #[must_use]
    pub const fn is_pipe(&self) -> bool {
        self.file_type.is_pipe()
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
