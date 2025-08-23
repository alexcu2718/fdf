use crate::{
    BytePath as _, DirIter, OsBytes, Result, custom_types_result::BytesStorage, filetype::FileType,
};

use std::{ffi::OsStr, os::unix::ffi::OsStrExt as _};

/// A struct representing a directory entry with minimal memory overhead.
///
/// This struct is designed for high-performance file system traversal and analysis.
/// It holds metadata and a path to a file or directory, optimised for size
/// and efficient access to its components.
///
/// # Type Parameters
///
/// - `S`: The storage type for the path bytes (e.g., `Box<[u8]>`, `Arc<[u8]>`, `Vec<u8>`).
///
/// # Memory Layout
///
/// The struct's memory footprint is approximately 10 bytes on most Unix-like platforms(MacOS/Linux):
/// - **Path**: A thin pointer wrapper, optimised for size.
/// - **File type**: A 1-byte enum representing the entry's type (file, directory, etc.).
/// - **Inode**: An 8-byte integer for the file's unique inode number.
/// - **Depth**: A 2-byte integer indicating the entry's depth from the root.
/// - **File name index**: A 2-byte integer pointing to the start of the file name within the path buffer.
///
/// # Examples
///
/// ```
/// use fdf::DirEntry;
/// use std::path::Path;
/// use std::fs::File;
/// use std::io::Write;
/// use std::sync::Arc;
///
///
///
/// // Create a temporary directory for the test
/// let temp_dir = std::env::temp_dir();
///
/// let file_path = temp_dir.join("test_file.txt");
///
/// // Create a file inside the temporary directory
/// {
///     let mut file = File::create(&file_path).expect("Failed to create file");
///     writeln!(file, "Hello, world!").expect("Failed to write to file");
/// }
///
/// // Create a DirEntry from the temporary file path
///  let entry: DirEntry<Arc<[u8]>>  = DirEntry::new(&file_path).unwrap();
/// assert!(entry.is_regular_file());
/// assert_eq!(entry.file_name(), b"test_file.txt");
///
///
/// ```
#[derive(Clone)] //could probably implement a more specialised clone.
pub struct DirEntry<S>
where
    S: BytesStorage,
{
    /// Path to the entry, stored as OS-native bytes.
    ///
    /// This is a thin pointer wrapper around the storage `S`, optimised for size (~10 bytes). ( on linux/macos, will be bigger on other systems)
    pub(crate) path: OsBytes<S>,

    /// File type (file, directory, symlink, etc.).
    ///
    /// Stored as a 1-byte enum.
    pub(crate) file_type: FileType,

    /// Inode number of the file.
    pub(crate) inode: u64,
    //
    /// Depth of the directory entry relative to the root.
    ///
    pub(crate) depth: u16, //2bytes

    /// Offset in the path buffer where the file name starts.
    ///
    /// This helps quickly extract the file name from the full path.
    pub(crate) file_name_index: u16,
    // 23 bytes in total if using slimmerbox (default for CLI on macos/Linux)
    //this is to save 1 byte to make sure the `Result`/`Option` enum is minimised (optimisation trick)
}

impl<S> DirEntry<S>
where
    S: BytesStorage,
{
    #[inline]
    #[must_use]
    /// Checks if the entry is an executable file.
    ///
    /// This is a **costly** operation as it performs an `access` system call.
    ///
    /// # Examples
    ///
    /// ```
    /// use fdf::DirEntry;
    /// use std::fs::{self, File};
    /// use std::os::unix::fs::PermissionsExt;
    /// use std::sync::Arc;
    /// let temp_dir = std::env::temp_dir();
    /// let exe_path = temp_dir.join("my_executable");
    /// File::create(&exe_path).unwrap().set_permissions(fs::Permissions::from_mode(0o755)).unwrap();
    ///
    /// let entry: DirEntry<Arc<[u8]>> = DirEntry::new(&exe_path).unwrap();
    /// assert!(entry.is_executable());
    ///
    /// let non_exe_path = temp_dir.join("my_file");
    /// File::create(&non_exe_path).unwrap();
    /// let non_exe_entry:DirEntry<Arc<[u8]>> = DirEntry::new(&non_exe_path).unwrap();
    /// assert!(!non_exe_entry.is_executable());
    /// fs::remove_file(exe_path).unwrap();
    /// fs::remove_file(non_exe_path).unwrap();
    /// ```
    pub fn is_executable(&self) -> bool {
        //X_OK is the execute permission, requires access call
        self.is_regular_file()
            && unsafe { self.as_cstr_ptr(|ptr| libc::access(ptr, libc::X_OK) == 0i32) }
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
    #[must_use]
    #[allow(clippy::wildcard_enum_match_arm)]
    /// Checks if the entry is empty.
    ///
    /// For files, it checks if the size is zero. For directories, it checks if there are no entries.
    /// This is a **costly** operation as it requires system calls (`stat` or `getdents`/`readdir`).
    ///
    /// # Examples
    ///
    /// ```
    /// use fdf::DirEntry;
    /// use std::fs::{self, File};
    /// use std::io::Write;
    /// use std::sync::Arc;
    /// let temp_dir = std::env::temp_dir();
    ///
    /// // Check an empty file.
    /// let empty_file_path = temp_dir.join("empty.txt");
    /// File::create(&empty_file_path).unwrap();
    /// let empty_entry:DirEntry<Arc<[u8]>> = DirEntry::new(&empty_file_path).unwrap();
    /// assert!(empty_entry.is_empty());
    ///
    /// // Check a non-empty file.
    /// let non_empty_file_path = temp_dir.join("not_empty.txt");
    /// File::create(&non_empty_file_path).unwrap().write_all(b"Hello").unwrap();
    /// let non_empty_entry:DirEntry<Arc<[u8]>> = DirEntry::new(&non_empty_file_path).unwrap();
    /// assert!(!non_empty_entry.is_empty());
    ///
    /// // Check an empty directory.
    /// let empty_dir_path = temp_dir.join("empty_dir");
    /// fs::create_dir(&empty_dir_path).unwrap();
    /// let empty_dir_entry:DirEntry<Arc<[u8]>> = DirEntry::new(&empty_dir_path).unwrap();
    /// assert!(empty_dir_entry.is_empty());
    ///
    /// fs::remove_file(empty_file_path).unwrap();
    /// fs::remove_file(non_empty_file_path).unwrap();
    /// fs::remove_dir(empty_dir_path).unwrap();
    /// ```
    pub fn is_empty(&self) -> bool {
        match self.file_type() {
            FileType::RegularFile => self.size().is_ok_and(|size| size == 0),
            FileType::Directory => {
                #[cfg(target_os = "linux")]
                let result = self
                    .getdents() //use getdents on linux to avoid  extra stat calls
                    .is_ok_and(|mut entries| entries.next().is_none());
                #[cfg(not(target_os = "linux"))]
                let result = self
                    .readdir()
                    .is_ok_and(|mut entries| entries.next().is_none());
                result
            }
            _ => false,
        }
    }
    #[inline]
    #[allow(clippy::missing_errors_doc)]
    /// Converts a directory entry to a full, canonical path, resolving all symlinks.
    ///
    /// This is a **costly** operation as it involves a system call (`realpath`).
    ///
    /// # Errors
    ///
    /// Returns an `Err`
    ///
    /// It can be one of the following,
    ///  `FileType::AccessDenied`, (EACCESS)
    ///  `FileType::TooManySymbolicLinks`, ( ELOOP)
    ///  `FileType::InvalidPath` (ENOENT)
    /// (There may be more, this documentation is not complete)
    //TODO!
    ///
    ///    
    ///
    /// # Examples
    ///
    /// ```ignore //these tests are broken on macos because of funky stuff I don't understand with tmp, which is annoying!
    /// use fdf::DirEntry;
    /// use std::path::Path;
    /// use std::fs;
    /// use std::sync::Arc;
    /// use std::os::unix::ffi::OsStrExt as _;
    /// use std::os::unix::fs::symlink;
    /// let temp_dir = std::env::temp_dir();
    /// let target_path = temp_dir.join("target_file_full_path.txt");
    /// fs::File::create(&target_path).unwrap();
    /// let symlink_path = temp_dir.join("link_to_target_full_path.txt");
    /// symlink(&target_path, &symlink_path).unwrap();
    ///
    /// // Create a DirEntry from the symlink path
    /// let entry: DirEntry<Arc<[u8]>> = DirEntry::new(&symlink_path).unwrap();
    /// assert!(entry.is_symlink());
    ///
    /// // Canonicalise the path
    /// let full_entry = entry.to_full_path().unwrap();
    ///
    /// // The full path of the canonicalised entry should match the target path.
    /// assert_eq!(full_entry.as_bytes(), target_path.as_os_str().as_bytes());
    ///
    /// fs::remove_file(&symlink_path).unwrap();
    /// fs::remove_file(&target_path).unwrap();
    /// ```
    pub fn to_full_path(self) -> Result<Self> {
        // SAFETY: the filepath must be less than `LOCAL_PATH_MAX` (default, 4096/1024 (System dependent))  (PATH_MAX but can be setup via envvar for testing)
        let ptr = unsafe {
            self.as_cstr_ptr(|cstrpointer| libc::realpath(cstrpointer, core::ptr::null_mut())) //we've created this pointer, we need to be careful
        };

        if ptr.is_null() {
            //check for null
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: pointer is guaranteed null terminated by the kernel, the pointer is properly aligned
        let full_path = unsafe { &*core::ptr::slice_from_raw_parts(ptr.cast(), libc::strlen(ptr)) }; //get length without null terminator (no ub check, this is why i do it this way)
        // we're dereferencing a valid pointer here, it's fine.
        //alignment is trivial, we use `libc::strlen` because it's probably the most optimal for possibly long paths
        // unfortunately my asm implementation doesn't perform well on long paths, which i want to figure out why(curiosity, not pragmatism!)

        let boxed = Self {
            path: full_path.into(), //we're heap allocating here
            file_type: self.file_type,
            inode: self.inode,
            depth: self.depth,
            file_name_index: full_path.file_name_index() as _,
        }; //we need the length up to the filename INCLUDING
        //including for slash, so eg ../hello/etc.txt has total len 16, then its base_len would be 16-7=9bytes
        //so we subtract the filename length from the total length, probably could've been done more elegantly.
        //TBD? not imperative.
        unsafe { libc::free(ptr.cast()) } //see definition below to check std library implementation
        //free the pointer to stop leaking

        Ok(boxed)
    }
    /*


        https://github.com/rust-lang/rust/blob/master/library/std/src/sys/fs/unix.rs
    pub fn canonicalize(path: &CStr) -> io::Result<PathBuf> {
        let r = unsafe { libc::realpath(path.as_ptr(), ptr::null_mut()) };
        if r.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(PathBuf::from(OsString::from_vec(unsafe {
            let buf = CStr::from_ptr(r).to_bytes().to_vec();
            libc::free(r as *mut _);
            buf
        })))
    }


        */

    #[inline]
    #[allow(clippy::missing_const_for_fn)]
    //this cant be const clippy be LYING AGAIN, this cant be const with slimmer box as it's misaligned,
    //so in my case, because it's 10 bytes, we're looking for an 8 byte reference, so it doesnt work
    #[must_use]
    ///Cost free conversion to bytes (because it is already is bytes)
    pub fn as_bytes(&self) -> &[u8] {
        self
    }

    #[inline]
    #[cfg(target_os = "linux")]
    pub fn to_temp_dirent(&self) -> crate::TempDirent<'_, S> {
        crate::TempDirent {
            path: self.path.as_bytes(),
            inode: self.inode,
            file_type: self.file_type,
            file_name_index: self.file_name_index as _,
            depth: self.depth as _,
            _marker: core::marker::PhantomData::<S>,
        }
    }

    #[inline]
    #[must_use]
    ///returns the file type of the file (eg directory, regular file, etc)
    pub const fn file_type(&self) -> FileType {
        self.file_type
    }

    #[inline]
    #[must_use]
    ///Returns the depth relative to the start directory, this is cost free
    pub const fn depth(&self) -> usize {
        self.depth as _
    }

    #[inline]
    #[must_use]
    ///Returns the name of the file (as bytes)
    pub fn file_name(&self) -> &[u8] {
        unsafe { self.get_unchecked(self.file_name_index()..) }
    }

    #[inline]
    #[must_use]
    ///returns the inode number of the file, cost free check
    ///
    ///
    /// this is a unique identifier for the file on the filesystem, it is not the same
    /// as the file name or path, it is a number that identifies the file on the
    /// It should be u32 on BSD's but I use u64 for consistency across platforms
    pub const fn ino(&self) -> u64 {
        self.inode //illumos/solaris doesnt have inode revisit when doing compatibility TODO!
    }

    #[inline]
    #[must_use]
    ///Applies a filter condition
    pub fn filter<F: Fn(&Self) -> bool>(&self, func: F) -> bool {
        func(self)
    }

    #[inline]
    #[must_use]
    ///returns the length of the base path (eg /home/user/ is 6 '/home/')
    pub const fn file_name_index(&self) -> usize {
        self.file_name_index as _
    }

    #[inline]
    #[must_use]
    ///Checks if the file is a directory or symlink, this is a cost free check
    pub const fn is_traversible(&self) -> bool {
        //this is a cost free check, we just check if the file type is a directory or symlink
        matches!(self.file_type, FileType::Directory | FileType::Symlink)
    }

    #[inline]
    #[must_use]
    ///checks if the file is hidden eg .gitignore
    pub fn is_hidden(&self) -> bool {
        unsafe { *self.get_unchecked(self.file_name_index()) == b'.' } //we use the base_len as a way to index to filename immediately, this means
        //we can store a full path and still get the filename without copying.
        //this is safe because we know that the base_len is always less than the length of the path
    }
    #[inline]
    #[must_use]
    ///returns the directory name of the file (as bytes) or failing that (/ is problematic) will return the full path,
    pub fn dirname(&self) -> &[u8] {
        unsafe {
            self //this is why we store the baseline, to check this and is hidden as above, its very useful and cheap
                .get_unchecked(..self.file_name_index() - 1)
                .rsplit(|&b| b == b'/')
                .next()
                .unwrap_or(self.as_bytes())
        }
    }

    #[inline]
    #[must_use]
    ///returns the parent directory of the file (as bytes)
    pub fn parent(&self) -> &[u8] {
        unsafe { self.get_unchecked(..core::cmp::max(self.file_name_index() - 1, 1)) }

        //we need to be careful if it's root,im not a fan of this method but eh.
        //theres probably a more elegant way. TODO!
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    ///Creates a new `DirEntry` from a path
    /// Rreturns a `Result<DirEntry, DirEntryError>`.
    /// This will error if path isn't valid/permission problems etc.
    pub fn new<T: AsRef<OsStr>>(path: T) -> Result<Self> {
        let path_ref = path.as_ref().as_bytes();

        // extract information from successful stat
        let get_stat = path_ref.get_lstat()?;
        let inode = access_stat!(get_stat, st_ino);
        Ok(Self {
            path: path_ref.into(),
            file_type: get_stat.into(),
            inode,
            depth: 0,
            file_name_index: path_ref.file_name_index(),
        })
    }

    /// Returns an iterator over directory entries using the `readdir` API.
    ///
    /// This provides a higher-level, more portable interface for directory iteration
    /// compared to `getdents`. Suitable for most use cases where maximum performance
    /// isn't critical.
    ///
    /// # Errors
    ///
    /// Returns `Err` if:
    /// - The entry is not a directory
    /// - Permission restrictions prevent reading the directory
    /// - The directory has been removed or become inaccessible
    /// - Any other system error occurs during directory opening/reading
    ///
    /// # Examples
    ///
    /// ```
    /// use fdf::DirEntry;
    /// use std::fs::{self, File};
    /// use std::io::Write;
    /// use std::sync::Arc;
    ///
    /// // Create a temporary directory with test files
    /// let temp_dir = std::env::temp_dir().join("test_readdir");
    /// fs::create_dir(&temp_dir).unwrap();
    ///
    /// // Create test files
    /// File::create(temp_dir.join("file1.txt")).unwrap().write_all(b"test").unwrap();
    /// File::create(temp_dir.join("file2.txt")).unwrap().write_all(b"test").unwrap();
    /// fs::create_dir(temp_dir.join("subdir")).unwrap();
    ///
    /// // Create DirEntry for the temporary directory
    /// let entry: DirEntry<Arc<[u8]>> = DirEntry::new(&temp_dir).unwrap();
    ///
    /// // Use readdir to iterate through directory contents
    /// let mut entries: Vec<_> = entry.readdir().unwrap().collect();
    /// entries.sort_by_key(|e| e.file_name().to_vec());
    ///
    /// // Should contain 3 entries: 2 files and 1 directory
    /// assert_eq!(entries.len(), 3);
    /// assert!(entries.iter().any(|e| e.file_name() == b"file1.txt"));
    /// assert!(entries.iter().any(|e| e.file_name() == b"file2.txt"));
    /// assert!(entries.iter().any(|e| e.file_name() == b"subdir"));
    /// fs::remove_dir_all(&temp_dir).unwrap();
    /// ```
    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn readdir(&self) -> Result<impl Iterator<Item = Self>> {
        DirIter::new(self)
    }
    #[inline]
    #[allow(clippy::missing_errors_doc)] //fixing errors l
    #[allow(clippy::cast_possible_truncation)] // truncation not a concern
    #[cfg(target_os = "linux")]

    /// Low-level directory iterator using the `getdents64` system call.
    ///
    /// This method provides high-performance directory scanning by using a large buffer
    /// (typically ~4.1KB) to minimise system calls. It's Linux-specific and generally
    /// faster than `readdir` for bulk directory operations.
    ///
    /// # Errors
    ///
    /// Returns `Err` if:
    /// - The entry is not a directory
    /// - Permission restrictions prevent reading the directory
    /// - The directory file descriptor cannot be opened
    /// - Buffer allocation fails
    /// - Any other system error occurs during the `getdents` operation
    ///
    /// # Platform Specificity
    ///
    /// This method is only available on Linux targets due to its dependence on
    /// the `getdents64` system call.
    ///
    /// # Examples
    ///
    /// ```
    /// use fdf::DirEntry;
    /// use std::fs::{self, File};
    /// use std::io::Write;
    /// use std::sync::Arc;
    ///
    /// // Create a temporary directory with test files
    /// let temp_dir = std::env::temp_dir().join("test_getdents");
    /// fs::create_dir(&temp_dir).unwrap();
    ///
    /// // Create test files
    /// File::create(temp_dir.join("file1.txt")).unwrap().write_all(b"test").unwrap();
    /// File::create(temp_dir.join("file2.txt")).unwrap().write_all(b"test").unwrap();
    /// fs::create_dir(temp_dir.join("subdir")).unwrap();
    ///
    /// // Create DirEntry for the temporary directory
    /// let entry: DirEntry<Arc<[u8]>> = DirEntry::new(&temp_dir).unwrap();
    ///
    /// // Use getdents to iterate through directory contents
    /// let mut entries: Vec<_> = entry.getdents().unwrap().collect();
    /// entries.sort_by_key(|e| e.file_name().to_vec());
    ///
    /// // Should contain 3 entries: 2 files and 1 directory
    /// assert_eq!(entries.len(), 3);
    /// assert!(entries.iter().any(|e| e.file_name() == b"file1.txt"));
    /// assert!(entries.iter().any(|e| e.file_name() == b"file2.txt"));
    /// assert!(entries.iter().any(|e| e.file_name() == b"subdir"));
    ///
    /// // Clean up
    /// fs::remove_dir_all(&temp_dir).unwrap();
    /// ```
    pub fn getdents(&self) -> Result<impl Iterator<Item = Self>> {
        use crate::iter::DirEntryIterator;
        let fd = unsafe { self.open_fd()? }; //returns none if null (END OF DIRECTORY/Directory no longer exists) (we've already checked if it's a directory/symlink originally )
        let mut path_buffer = crate::AlignedBuffer::<u8, { crate::LOCAL_PATH_MAX }>::new(); //nulll initialised  (stack) buffer that can axiomatically hold any filepath.
        let path_len = unsafe { path_buffer.init_from_direntry(self) };
        //TODO! make this more ergonomic

        Ok(DirEntryIterator {
            fd,
            buffer: crate::SyscallBuffer::new(),
            path_buffer,
            file_name_index: path_len as _,
            parent_depth: self.depth,
            offset: 0,
            remaining_bytes: 0,
            _marker: core::marker::PhantomData::<S>, // marker for the storage type, this is used to ensure that the iterator can be used with any storage type
        })
    }
}
