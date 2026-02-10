/*!
 A high-performance, parallel directory traversal and file search library.

 This library provides efficient file system traversal with features including:
 - Parallel directory processing using Rayon
 - Low-level system calls for optimal performance on supported platforms
 - Flexible filtering by name, size, type, and custom criteria
 - Symbolic link handling with cycle detection
 - Cross-platform support with platform-specific optimisations
 - Provides easy access to `CStr` for FFI use.

 # Examples
 Simple file search example
 ```no_run
 use fdf::{walk::Finder, SearchConfig};
 use std::error::Error;
 use std::sync::Arc;

fn main() -> Result<(), Box<dyn Error>> {
    let finder = Finder::init("/some/path")
        .pattern("*.txt")
        .build()
        .expect("Failed to build finder");

    let entries = finder
        .traverse()
        .expect("Failed to start traversal");

    let mut file_count = 0;

    for entry in entries {
        file_count += 1;
        println!("Found: {:?}", entry);
    }

    println!("Discovered {} files", file_count);

    Ok(())
}

 ```

 Advanced file search example
 ```no_run
use fdf::{walk::Finder, filters::{SizeFilter, FileTypeFilter}};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let finder = Finder::init("/some/path")
        .pattern("*.txt")
        .keep_hidden(false)
        .case_insensitive(true)
        .keep_dirs(true)
        .max_depth(Some(5))
        .follow_symlinks(false)
        .filter_by_size(Some(SizeFilter::Min(1024)))
        .type_filter(Some(FileTypeFilter::File))
        .collect_errors(true) // Gather the errors from iteration, this has some performance cost.
        .build()
        .map_err(|e| format!("Failed to build finder: {}", e))?;

    let entries = finder
        .traverse()
        .map_err(|e| format!("Failed to start traversal: {}", e))?;

    let mut file_count = 0;

    for entry in entries {
        file_count += 1;
        println!("{:?}", entry);
    }

    println!("Search completed! Found {} files", file_count);

    Ok(())
}
```

*/

use crate::fs::ReadDir;
use crate::fs::{FileDes, FileType, types::Result};
use crate::{DirEntryError, util::BytePath as _};
use chrono::{DateTime, Utc};
use core::cell::Cell;
use core::ffi::CStr;
use core::ffi::c_char;
use core::fmt;
use core::ptr::NonNull;

use libc::{
    AT_SYMLINK_FOLLOW, AT_SYMLINK_NOFOLLOW, F_OK, R_OK, W_OK, X_OK, access, fstatat, lstat,
    realpath, stat,
};
use std::{ffi::OsStr, os::unix::ffi::OsStrExt as _, path::Path};

/**
  A struct representing a directory entry with minimal memory overhead.

  This struct is designed for high-performance file system traversal and analysis.
  It holds metadata and a path to a file or directory, optimised for size
  and efficient access to its components.

  The struct's memory footprint is
  - **Path**: 16 bytes, `Box<CStr>`, retaining compatibility for use in libc but converting to `&[u8]` trivially(as deref)
  - **File type**:  1-byte enum representing the entry's type (file, directory, etc.).
  - **Inode**:  8-byte integer for the file's unique inode number.
  - **Depth**:  4-byte integer indicating the entry's depth from the root.
  - **File name index**:  8-byte integer pointing to the start of the file name within the path buffer.
  - **is traversible cache**:  1 byte `Cell<Option<bool>>` an Implementation detail that avoids recalling stat on symlinks

  # Examples

    ```
    use fdf::fs::DirEntry;
    use std::path::Path;
    use std::fs::File;
    use std::io::Write;
    use std::sync::Arc;



    // Create a temporary directory for the test
    let temp_dir = std::env::temp_dir();


    let file_path = temp_dir.join("test_file.txt");

    // Create a file inside the temporary directory

    let mut file = File::create(&file_path).expect("Failed to create file");
    writeln!(file, "Hello, world!").expect("Failed to write to file");


    // Create a DirEntry from the temporary file path
    let entry = DirEntry::new(&file_path).unwrap();
    assert!(entry.is_regular_file());
    assert_eq!(entry.file_name(), b"test_file.txt");
    ```
*/
#[derive(Clone)] //could probably implement a more specialised clone.
pub struct DirEntry {
    /// Path to the entry, stored as a Boxed `CStr`
    /// This allows easy C ffi by just calling `.as_ptr()`
    //(to avoid storing the capacity, since the path is immutable once set)
    pub(crate) path: Box<CStr>, //16 bytes
    //TODO rewrite CStr manually  due to this FIXME
    /* https://doc.rust-lang.org/src/core/ffi/c_str.rs.html#103
    // FIXME: this should not be represented with a DST slice but rather with
    //        just a raw `c_char` along with some form of marker to make
    //        this an unsized type. Essentially `sizeof(&CStr)` should be the
    //        same as `sizeof(&c_char)` but `CStr` should be an unsized type.
     */
    /// File type (file, directory, symlink, etc.).
    pub(crate) file_type: FileType, //1 byte

    /// Inode number of the file.
    pub(crate) inode: u64, //8 bytes
    /// Depth of the directory entry relative to the root.
    pub(crate) depth: u32, //4bytes

    /// Offset in the path buffer where the file name starts.
    ///
    /// This helps quickly extract the file name from the full path.
    pub(crate) file_name_index: usize, //8 bytes
    ///
    /// `None` means not computed yet, `Some(bool)` means cached result.
    pub(crate) is_traversible_cache: Cell<Option<bool>>, //1byte
} //38 bytes, rounded to 40

impl core::ops::Deref for DirEntry {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsRef<[u8]> for DirEntry {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<'drnt> From<&'drnt DirEntry> for &'drnt CStr {
    #[inline]
    fn from(entry: &'drnt DirEntry) -> &'drnt CStr {
        &entry.path
    }
}

impl fmt::Display for DirEntry {
    // TODO: I might need to change this to show other metadata.
    #[allow(clippy::missing_inline_in_public_items)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_lossy())
    }
}

impl From<DirEntry> for std::path::PathBuf {
    #[inline]
    fn from(entry: DirEntry) -> Self {
        entry.as_os_str().into()
    }
}
impl TryFrom<&[u8]> for DirEntry {
    type Error = DirEntryError;

    #[inline]
    fn try_from(path: &[u8]) -> Result<Self> {
        Self::new(OsStr::from_bytes(path))
    }
}

impl TryFrom<&OsStr> for DirEntry {
    type Error = DirEntryError;

    #[inline]
    fn try_from(path: &OsStr) -> Result<Self> {
        Self::new(path)
    }
}

impl AsRef<Path> for DirEntry {
    #[inline]
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl fmt::Debug for DirEntry {
    #[allow(clippy::missing_inline_in_public_items)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dirent")
            .field("path", &self.to_string_lossy())
            .field("file_name", &String::from_utf8_lossy(self.file_name()))
            .field("depth", &self.depth)
            .field("file_type", &self.file_type)
            .field("file_name_index", &self.file_name_index)
            .field("inode", &self.inode)
            .field("traversible_cache", &self.is_traversible_cache)
            .finish()
    }
}

impl DirEntry {
    /**
    Checks if the entry is an executable file.

     This is a **costly** operation as it performs an `access` system call.

    # Examples

    ```
      use fdf::fs::DirEntry;
      use std::fs::{self, File};
      use std::os::unix::fs::PermissionsExt;
      use std::sync::Arc;

      let temp_dir = std::env::temp_dir();
      let exe_path = temp_dir.join("my_executable");
      File::create(&exe_path).unwrap().set_permissions(fs::Permissions::from_mode(0o755)).unwrap();

      let entry = DirEntry::new(&exe_path).unwrap();
      assert!(entry.is_executable());

      let non_exe_path = temp_dir.join("my_file");
      File::create(&non_exe_path).unwrap();
      let non_exe_entry = DirEntry::new(&non_exe_path).unwrap();
      assert!(!non_exe_entry.is_executable());
      fs::remove_file(exe_path).unwrap();
      fs::remove_file(non_exe_path).unwrap();
      ```
    */
    #[inline]
    pub fn is_executable(&self) -> bool {
        // X_OK is the execute permission, requires access call
        // SAFETY: We know the path is valid because internally it's a cstr
        self.is_regular_file() && unsafe { access(self.as_ptr(), X_OK) == 0 }
    }

    /**
     Returns a raw pointer to the underlying C string.

     This provides access to the null-terminated C string representation
     of the file path for use with FFI functions.
    */
    #[inline]
    pub const fn as_ptr(&self) -> *const c_char {
        self.path.as_ptr() //Have to explicitly override the default deref to bytes
    }

    /// Returns the underlying path as a `Path`
    #[inline]
    pub const fn as_path(&self) -> &Path {
        // SAFETY: bytes <=> OsStr <=> Path on unix
        unsafe { core::mem::transmute(self.as_bytes()) } //admittedly this and below are a const hack. Why not make it const if you can?
    }

    /// Returns the underlying path as an `OsStr`
    #[inline]
    pub const fn as_os_str(&self) -> &OsStr {
        // SAFETY: bytes <=> OsStr <=> Path on unix
        unsafe { core::mem::transmute(self.as_bytes()) }
    }

    /// Cost free check for block devices
    #[inline]
    #[must_use]
    pub const fn is_block_device(&self) -> bool {
        self.file_type.is_block_device()
    }

    #[inline]
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
    /**
     Opens the directory and returns a file descriptor.

     This is a low-level operation that opens the directory with the following flags:
     - `O_CLOEXEC`: Close the file descriptor on exec
     - `O_DIRECTORY`: Fail if not a directory
     - `O_NONBLOCK`: Open in non-blocking mode

    */
    pub(crate) fn open(&self) -> Result<FileDes> {
        // Opens the file and returns a file descriptor..
        const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK;
        // SAFETY: the pointer is null terminated
        let fd = unsafe { libc::open(self.as_ptr(), FLAGS) };

        if fd < 0 {
            return_os_error!()
        }
        Ok(FileDes(fd))
    }

    /*
    Commented out temporarily while I work on API
    /**
     Opens the directory relative to a directory file descriptor and returns a file descriptor.

     This function uses the `openat` system call to open a directory relative to an already open
     directory file descriptor. This is useful for avoiding race conditions that can occur when
     using absolute paths, as it ensures the operation is performed relative to a specific directory.

     The directory is opened with the following flags:
     - `O_CLOEXEC`: Close the file descriptor on exec
     - `O_DIRECTORY`: Fail if not a directory
     - `O_NONBLOCK`: Open in non-blocking mode

     # Examples

     ## Basic usage

     ```
     use fdf::{DirEntry, FileDes, Result};
     use std::fs;
     use std::env::temp_dir;

     # fn main() -> Result<()> {
     // Create a temporary directory for testing
     let temp_dir = temp_dir().join("fdf_openat_test");
     let _ = fs::remove_dir_all(&temp_dir);
     fs::create_dir_all(&temp_dir)?;

     // Create a subdirectory inside the temp directory
     let subdir_path = temp_dir.join("test_subdir");
     fs::create_dir(&subdir_path)?;

     // Open the temporary directory
     let temp_dir_entry = DirEntry::new(&temp_dir)?;
     let temp_fd = temp_dir_entry.open()?;

     // Use openat to open the subdirectory relative to the temp directory
     let subdir_entry = DirEntry::new(&subdir_path)?;
     let subdir_fd = subdir_entry.openat(&temp_fd)?;

     // Clean up
     let _ = fs::remove_dir_all(&temp_dir);
     # Ok(())
     # }
     ```
     # Platform-specific behavior

     - On Linux, this uses the `openat` system call directly
     - The behavior may vary on other Unix-like systems, but the interface is standardized in POSIX

     # Errors

     Returns an error if:
     - The directory doesn't exist or can't be opened
     - The path doesn't point to a directory (fails due to `O_DIRECTORY` flag)
     - Permission is denied for the directory
     - The parent file descriptor (`dir_fd`) is invalid or not open
     - The path contains null bytes
     - System resources are exhausted (too many open file descriptors)

     # See also

     - [openat(2) - Linux man page](https://man7.org/linux/man-pages/man2/openat.2.html)
     - [`DirEntry::open`] for opening directories with absolute paths
    */
    #[inline]
    pub fn openat(&self, fd: &FileDes) -> Result<FileDes> {
        // Opens the file and returns a file descriptor.
        // This is a low-level operation that may fail if the file does not exist or cannot be opened.
        const FLAGS: i32 = O_CLOEXEC | O_DIRECTORY | O_NONBLOCK;
        // SAFETY: the pointer is null terminated
        let filedes = unsafe { openat(fd.0, self.file_name_cstr().as_ptr(), FLAGS) };

        if filedes < 0 {
            return_os_error!()
        }
        Ok(FileDes(filedes))
    }*/

    /**  Opens a directory stream for reading directory entries.

    This function returns a `NonNull<DIR>` pointer to the directory stream,
    which can be used with `readdir` to iterate over directory entries.


    # Errors

    Returns an error if:
    - The directory doesn't exist or can't be opened
    - The path doesn't point to a directory
    - Permission is denied
    - System resources are exhausted
    */
    #[inline]
    pub(crate) fn opendir(&self) -> Result<NonNull<libc::DIR>> {
        // SAFETY: we are passing a null terminated directory to opendir

        let dir = unsafe { libc::opendir(self.as_ptr()) };
        // This function reads the directory entries and populates the iterator.
        // It is called when the iterator is created or when it needs to be reset.
        if dir.is_null() {
            return_os_error!()
        }
        // SAFETY: know it's non-null
        Ok(unsafe { NonNull::new_unchecked(dir) }) // Return a pointer to the start `DIR` stream
    }

    // Converts to a lossy string for ease of use
    #[inline]
    #[must_use]
    pub fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(self)
    }

    /**
    Returns the underlying bytes as a UTF-8 string slice if valid.
    # Errors
    Returns `Err` if the bytes are not valid UTF-8.
    */
    #[inline]
    pub const fn as_str(&self) -> core::result::Result<&str, core::str::Utf8Error> {
        core::str::from_utf8(self.as_bytes())
    }

    /// Cost free check for character devices
    #[inline]
    #[must_use]
    pub const fn is_char_device(&self) -> bool {
        self.file_type.is_char_device()
    }

    /// Cost free check for pipes (FIFOs)
    #[inline]
    #[must_use]
    pub const fn is_pipe(&self) -> bool {
        self.file_type.is_pipe()
    }

    /// Cost free check for sockets
    #[inline]
    #[must_use]
    pub const fn is_socket(&self) -> bool {
        self.file_type.is_socket()
    }

    /// Cost free check for regular files
    #[inline]
    #[must_use]
    pub const fn is_regular_file(&self) -> bool {
        self.file_type.is_regular_file()
    }

    /// Cost free check for directories
    #[inline]
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        self.file_type.is_dir()
    }

    /// Cost free check for unknown file types
    #[inline]
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        self.file_type.is_unknown()
    }

    /// Cost free check for symlinks
    #[inline]
    #[must_use]
    pub const fn is_symlink(&self) -> bool {
        self.file_type.is_symlink()
    }

    /**
    Checks if the entry is empty.

    For files, it checks if the size is zero. For directories, it checks if there are no entries.
    This is a **costly** operation as it requires system calls (`stat` or `getdents`/`readdir`).

    # Examples

    ```
    use fdf::fs::DirEntry;
    use std::fs::{self, File};
    use std::io::Write;
    use std::sync::Arc;
    let temp_dir = std::env::temp_dir();

    // Check an empty file.
    let empty_file_path = temp_dir.join("empty.txt");
    File::create(&empty_file_path).unwrap();
    let empty_entry = DirEntry::new(&empty_file_path).unwrap();
    assert!(empty_entry.is_empty());

    // Check a non-empty file.
    let non_empty_file_path = temp_dir.join("not_empty.txt");
    File::create(&non_empty_file_path).unwrap().write_all(b"Hello").unwrap();
    let non_empty_entry = DirEntry::new(&non_empty_file_path).unwrap();
    assert!(!non_empty_entry.is_empty());

    // Check an empty directory.
    let empty_dir_path = temp_dir.join("empty_dir");
    fs::create_dir(&empty_dir_path).unwrap();
    let empty_dir_entry = DirEntry::new(&empty_dir_path).unwrap();
    assert!(empty_dir_entry.is_empty());

    fs::remove_file(empty_file_path).unwrap();
    fs::remove_file(non_empty_file_path).unwrap();
    fs::remove_dir(empty_dir_path).unwrap();
    ```
    */
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        // because calling getdents on an empty dir should only return 48 on these platforms, meaning we dont have
        // to allocate the vector in getdents, finding empty files is a good use case so even though it's niche
        // it would have appeal to people. Not too hard to implement.
        match self.file_type {
            FileType::RegularFile => self.file_size().is_ok_and(|size| size == 0),
            FileType::Directory => {
                #[cfg(any(target_os = "linux", target_os = "android"))]
                let result = self.is_empty_dir();
                #[cfg(not(any(target_os = "linux", target_os = "android")))]
                let result = self
                    .readdir()
                    .is_ok_and(|mut entries| entries.next().is_none());
                result
            }
            _ => false,
        }
    }

    #[inline]
    #[cfg(any(target_os = "linux", target_os = "android"))]
    /// Specialisation for empty checks on linux/android (avoid a heap alloc)
    pub(crate) fn is_empty_dir(&self) -> bool {
        use crate::fs::AlignedBuffer;
        use crate::util::getdents;
        const BUF_SIZE: usize = 500; //pretty arbitrary.
        #[allow(clippy::cast_possible_wrap)] // need to match i64 semantics(doesnt matter)
        const MINIMUM_DIRENT_SIZE: isize =
            core::mem::offset_of!(crate::dirent64, d_name).next_multiple_of(8) as _;
        debug_assert!(
            self.file_type == FileType::Directory || self.file_type == FileType::Symlink,
            " Only expect dirs/symlinks to pass through this private func"
        );
        let dirfd = self.open();
        if let Ok(fd) = dirfd {
            let mut syscall_buffer = AlignedBuffer::<u8, BUF_SIZE>::new();
            // SAFETY: guaranteed open, valid ptr etc.
            let dents = unsafe { getdents(fd.0, syscall_buffer.as_mut_ptr(), BUF_SIZE) };

            // SAFETY: Closed only once confirmed open
            unsafe { libc::close(fd.0) };
            // if empty, then only 2 entries expected, . and .., this means only 48 or below (or neg if errors, who cares.)
            return dents <= 2 * MINIMUM_DIRENT_SIZE;
        }
        false //return false is open fails
    }

    /**
    Returns the full path of this directory entry as a `CStr`.

    This function provides direct access to the underlying null-terminated C string
    representing the entry’s absolute or relative path (depending on how it was created).

     The returned reference is valid for the lifetime of the `DirEntry` and does not allocate.

    # Examples

     ```
     use fdf::fs::DirEntry;
     use std::fs::File;
     use std::io::Write;
     use std::os::unix::ffi::OsStrExt;
    // Create a temporary file
    let tmp = std::env::temp_dir().join("as_cstr_test.txt");
     File::create(&tmp).unwrap().write_all(b"data").unwrap();

    // Create a DirEntry from the file path
    let entry = DirEntry::new(&tmp).unwrap();

     // Retrieve full path as a CStr
    let cstr = entry.as_cstr();

    // It should match the full path string plus null terminator
    let expected = std::ffi::CString::new(tmp.as_os_str().as_bytes()).unwrap();
     assert_eq!(cstr.to_bytes_with_nul(), expected.to_bytes_with_nul());

     // The CStr can be converted back to &str (if valid UTF-8)
    let path_str = cstr.to_str().unwrap();
    assert!(path_str.ends_with("as_cstr_test.txt"));
    std::fs::remove_file(tmp).unwrap();
    ```
    */
    #[inline]
    pub const fn as_cstr(&self) -> &CStr {
        &self.path
    }

    /**
     Returns the file name component of the entry as a `CStr`.

     This slice is a view into the internal path buffer and includes the null terminator.
     It is guaranteed to be valid UTF-8 if and only if the file name is UTF-8.

    Useful within the openat calls
     # Examples

     ```
     use fdf::fs::DirEntry;
     use std::fs::File;
     use std::io::Write;

     // Create a temporary file
     let tmp = std::env::temp_dir().join("file_name_cstr_test.txt");
     File::create(&tmp).unwrap().write_all(b"abc").unwrap();

     // Build DirEntry
    let entry = DirEntry::new(&tmp).unwrap();

    // Extract file name as CStr
    let cstr = entry.file_name_cstr();

    // It should match the file name plus null terminator
    assert_eq!(cstr.to_bytes_with_nul(), b"file_name_cstr_test.txt\0");

    // We can also convert it to a &str
    assert_eq!(cstr.to_str().unwrap(), "file_name_cstr_test.txt");

    std::fs::remove_file(tmp).unwrap();
    ```

    ```
    use fdf::fs::DirEntry;
    use std::fs::File;

     // File name with spaces
    let tmp = std::env::temp_dir().join("some name.txt");
    File::create(&tmp).unwrap();



    let entry = DirEntry::new(&tmp).unwrap();
    let name = entry.file_name_cstr();

    assert_eq!(name.to_str().unwrap(), "some name.txt");

    std::fs::remove_file(tmp).unwrap();




    let root_dir=DirEntry::new("/");

    assert!(root_dir.is_err() || root_dir.is_ok_and(|x| x.file_name_cstr()==c"/"));

    ```
    */
    #[inline]
    pub fn file_name_cstr(&self) -> &CStr {
        let bytes = self.path.to_bytes_with_nul();

        //SAFETY:
        // `file_name_index()` returns a valid index within `bytes` bounds
        // The slice from this index includes the terminating null byte
        //`bytes` contains no interior null bytes before the terminator
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            CStr::from_bytes_with_nul_unchecked(bytes.get_unchecked(self.file_name_index()..))
        }
    }

    /**
    Returns the name of the file (as bytes, no null terminator)
    ( Returns `/` or `.` when they are the entry)


    ```
    use fdf::fs::DirEntry;
    use std::fs::File;

    // File name with spaces
    let tmp = std::env::temp_dir().join("some name.txt");
    File::create(&tmp).unwrap();



    let entry = DirEntry::new(&tmp).unwrap();
    let name = entry.file_name();

    assert_eq!(name, b"some name.txt");

    std::fs::remove_file(tmp).unwrap();




    let root_dir=DirEntry::new("/");

    assert!(root_dir.is_err() || root_dir.is_ok_and(|x| x.file_name()==b"/"));
    // if on certain systems, root dir requires permissions, so we have to be careful (esp android)

    let dot_dir=DirEntry::new(".");
    assert!(dot_dir.is_err() ||dot_dir.is_ok_and(|x| x.file_name()==b"."));

    ```
    */
    #[inline]
    #[must_use]
    pub fn file_name(&self) -> &[u8] {
        debug_assert!(
            self.len() >= self.file_name_index(),
            "this should always be equal or below (equal only when root)"
        );
        // SAFETY: the index is below the length of the path trivially
        unsafe { self.get_unchecked(self.file_name_index()..) }
    }

    /// Takes the value of the path and gives the raw representation as a boxed Cstr
    #[inline]
    pub fn to_inner(self) -> Box<CStr> {
        self.path
    }

    /// Private function  because it invokes a closure (to avoid doubly allocating unnecessary)
    #[inline]
    #[allow(clippy::multiple_unsafe_ops_per_block)] //annoying
    pub(crate) fn get_realpath<F, T>(&self, f: F) -> Result<T>
    where
        F: Fn(&CStr) -> Result<T>,
    {
        // SAFETY: realpath mallocs a null-terminated string that must be freed, the pointer is null terminated
        let ptr = unsafe { realpath(self.as_ptr(), core::ptr::null_mut()) };

        if ptr.is_null() {
            return_os_error!()
        }

        // SAFETY: ptr is valid and points to a null-terminated C string
        let cstr = unsafe { CStr::from_ptr(ptr) };

        // Run supplied closure; allow propagation of Result errors
        let result = f(cstr);

        // SAFETY: ptr was allocated by realpath(), so we must free it explicitly(only after function is run)
        unsafe { libc::free(ptr.cast()) }

        result
    }

    /**
    Converts a directory entry to a full, canonical path, resolving all symlinks

    This is a **costly** operation as it involves a system call (`realpath`).
    If the filetype is a symlink, it invokes a stat call to find the realtype, otherwise stat is not called.

    # Errors

    Returns an `Err`

    It can be one of the following,
    `FileType::AccessDenied`, (EACCESS)
    `FileType::TooManySymbolicLinks`, ( ELOOP)
    `FileType::InvalidPath` (ENOENT)
    (There may be more, this documentation is not complete) TODO!



    # Examples
    these tests are broken on macos because of funky stuff  with mac's privacy/security settings.
    ```no_run

    use fdf::fs::DirEntry;
    use std::path::Path;
    use std::fs;
    use std::sync::Arc;
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::symlink;
    let temp_dir = std::env::temp_dir();
    let target_path = temp_dir.join("target_file_full_path.txt");
    fs::File::create(&target_path).unwrap();
    let symlink_path = temp_dir.join("link_to_target_full_path.txt");
    symlink(&target_path, &symlink_path).unwrap();

    // Create a DirEntry from the symlink path
    let entry = DirEntry::new(&symlink_path).unwrap();
    assert!(entry.is_symlink());

    // Canonicalise the path
    let full_entry = entry.to_full_path().unwrap();

    // The full path of the canonicalised entry should match the target path.
    assert_eq!(full_entry.as_bytes(), target_path.as_os_str().as_bytes());

    fs::remove_file(&symlink_path).unwrap();
    fs::remove_file(&target_path).unwrap();

    ```
    */
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    pub fn to_full_path(&self) -> Result<Self> {
        self.get_realpath(|full_path| {
            let file_name_index = full_path.to_bytes().file_name_index();

            let (file_type, ino) = if self.is_symlink() {
                let statted = self.get_stat()?; // only call stat if it's a symlink, because I don't deduplicate normal files, this works well for symlinks
                (FileType::from_stat(&statted), access_stat!(statted, st_ino))
            } else {
                (self.file_type(), self.ino())
            };

            Ok(Self {
                path: full_path.into(),
                file_type,
                inode: ino,
                depth: self.depth,
                file_name_index,
                is_traversible_cache: Cell::new(Some(file_type == FileType::Directory)),
            })
        })
    }
    /**
    Returns the parent path as byte slice, or None if at root.

    # Examples

    Basic usage with a temporary directory and file:

    ```
    use fdf::fs::DirEntry;
    use std::fs::File;
    use std::os::unix::ffi::OsStrExt;
    let tmp = std::env::temp_dir().join("fdf_parent_test_dir");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let file_path = tmp.join("file.txt");
    File::create(&file_path).unwrap();

    let entry = DirEntry::new(&file_path).unwrap();
    assert_eq!(entry.parent().unwrap(), tmp.as_os_str().as_bytes());

    std::fs::remove_file(&file_path).unwrap();
    std::fs::remove_dir_all(&tmp).unwrap();
    ```

    Root path yields `None` for parent (guarded to avoid failing on unusual systems):
    dot entries return an empty byte slice, mirroring semantics from stdlib.

    ```
    use fdf::fs::DirEntry;

    let root = DirEntry::new("/");
    if let Ok(e) = root {
        assert!(e.parent().is_none());
    }

    let dot=DirEntry::new(".");
    if let Ok(dotentry)=dot{
    let parent=dotentry.parent();
    assert!(dotentry.parent().is_some_and(|x| x.is_empty()));
    }

    ```
    */
    #[inline]
    pub fn parent(&self) -> Option<&[u8]> {
        self.as_path()
            .parent()
            .map(|path| path.as_os_str().as_bytes())
        // TODO rewrite this eventually
    }

    /**
    Checks if the file or directory is readable by the current process.

    This uses the `access` system call with `R_OK` to check read permissions
    without actually opening the file. It follows symlinks.

    # Returns

    `true` if the current process has read permission, `false` otherwise.

    `false` if the file doesn't exist or on permission errors.
    */
    #[inline]
    pub fn is_readable(&self) -> bool {
        // SAFETY: The path is guaranteed to be a be null terminated
        unsafe { access(self.as_ptr(), R_OK) == 0 }
    }

    /**
    Checks if the file or directory is writable by the current process.

    This uses the `access` system call with `W_OK` to check write permissions
    without actually opening the file. It follows symlinks.


    # Returns

    `true` if the current process has write permission, `false` otherwise.
    `false` if the file doesn't exist or on permission errors.
    */
    #[inline]
    pub fn is_writable(&self) -> bool {
        //maybe i can automatically exclude certain files from this check to
        //then reduce my syscall total, would need to read into some documentation. zadrot ebaniy
        // SAFETY: The path is guaranteed to be a null terminated
        unsafe { access(self.as_ptr(), W_OK) == 0 }
    }

    /**
    Checks if the file exists.

    This makes a system call to check file existence.
    */
    #[inline]
    pub fn exists(&self) -> bool {
        // SAFETY: The path is guaranteed to be null terminated
        unsafe { access(self.as_ptr(), F_OK) == 0 }
    }

    /**
    Gets file metadata using lstatat for a file relative to a directory file descriptor.

    This function uses `fstatat` with `AT_SYMLINK_NOFOLLOW` to get metadata without
    following symbolic links, similar to `lstat` but relative to a directory fd.

    # Arguments
    `fd` - Directory file descriptor to use as the base for relative path resolution

    # Returns
    A `stat` structure containing file metadata on success.

    # Errors
    Returns `DirEntryError::IOError` if the stat operation fails
    */
    #[inline]
    pub fn get_lstatat(&self, fd: &FileDes) -> Result<stat> {
        stat_syscall!(
            fstatat,
            fd.0,
            self.file_name_cstr().as_ptr(),
            AT_SYMLINK_NOFOLLOW
        )
    }

    /**

    Gets file status information without following symlinks.

    This performs an `lstat` system call to retrieve metadata about the file,
    including type, permissions, size, and timestamps. Unlike `stat`,
    `lstat` returns information about the symlink itself rather than the target.



    # Errors

    Returns an error if:
    - The file doesn't exist
    - Permission is denied
    - The path is invalid
    - System call fails for any other reason

    Returns `DirEntryError::IOError` if the stat operation fails

    A `stat` structure containing file metadata on success.
    */
    #[inline]
    pub fn get_lstat(&self) -> Result<stat> {
        stat_syscall!(lstat, self.as_ptr())
    }

    /**
    Gets file status information by following symlinks.

    This performs a `stat` system call to retrieve metadata about the file,
    including type, permissions, size, and timestamps. Unlike `lstat`,
    `stat` follows symbolic links and returns information about the target file
    rather than the link itself.


    # Errors

    Returns an error if:
    - The file doesn't exist
    - Permission is denied
    - The path is invalid
    - A symlink target doesn't exist
    - System call fails for any other reason

    #  Returns `DirEntryError::IOError` if the stat operation fails

    A `stat` structure containing file metadata on success.
    */
    #[inline]
    pub fn get_stat(&self) -> Result<stat> {
        // Simple wrapper to avoid code duplication so I can use the private method within the crate
        stat_syscall!(stat, self.as_ptr())
    }

    /**
    Gets file metadata using statat for a file relative to a directory file descriptor.

    This function uses `fstatat` with `AT_SYMLINK_FOLLOW` to get metadata by
    following symbolic links, similar to `stat` but relative to a directory fd.

    # Arguments
    `fd` - Directory file descriptor to use as the base for relative path resolution

    # Returns
    A `stat` structure containing file metadata on success.

    # Errors
    Returns `DirEntryError::IOError` if the stat operation fails

    */
    #[inline]
    pub fn get_statat(&self, fd: &FileDes) -> Result<stat> {
        stat_syscall!(
            fstatat,
            fd.0,
            self.file_name_cstr().as_ptr(),
            AT_SYMLINK_FOLLOW
        )
    }

    /// Cost free conversion to bytes (because it is already is bytes)
    #[inline]
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        self.path.to_bytes()
    }

    /**
    Returns the length of the path string in bytes.

    The length excludes the internal null terminator, so it matches
    what you'd expect from a regular Rust string slice length.

    # Examples
    ```
    use fdf::fs::DirEntry;
    use std::fs::File;

    let tmp = std::env::temp_dir().join("test_file.txt");
    File::create(&tmp).unwrap();

    let entry = DirEntry::new(&tmp).unwrap();
    // Length matches the path string without null terminator
    assert_eq!(entry.len(), tmp.as_os_str().len());

    std::fs::remove_file(tmp).unwrap();
    ```
    */
    #[inline]
    pub const fn len(&self) -> usize {
        self.path.count_bytes()
    }

    /// Returns the file type of the file (eg directory, regular file, etc)
    #[inline]
    #[must_use]
    pub const fn file_type(&self) -> FileType {
        self.file_type
    }

    ///Returns the depth relative to the start directory, this is cost free
    #[inline]
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth as _
    }

    /// Returns the inode number of the file, cost free check
    ///
    ///
    /// This is a unique identifier for the file on the filesystem, it is not the same
    /// as the file name or path, it is a number that identifies the file on the
    // It should be u32 on BSD's but I use u64 for consistency across platforms
    #[inline]
    #[must_use]
    pub const fn ino(&self) -> u64 {
        self.inode
    }

    /// Applies a filter condition
    #[inline]
    #[must_use]
    pub fn filter<F: Fn(&Self) -> bool>(&self, func: F) -> bool {
        func(self)
    }

    /// Returns the length of the base path (eg /home/user/ is 6 '/home/') and '/' is 0
    #[inline]
    #[must_use]
    pub const fn file_name_index(&self) -> usize {
        self.file_name_index
    }

    /// Checks if the file is a directory or symlink (but internally a directory)
    #[inline]
    #[must_use]
    pub fn is_traversible(&self) -> bool {
        match self.file_type {
            FileType::Directory => true,
            FileType::Symlink => self.check_symlink_traversibility(), // we essentially use a cache to avoid recalling this for subsequent calls, see below.
            _ => false,
        }
    }

    /// Checks if a symlink points to a traversible directory, caching the result.
    #[inline]
    pub(crate) fn check_symlink_traversibility(&self) -> bool {
        debug_assert!(
            self.file_type() == FileType::Symlink,
            "we only expect symlinks to use this function(hence private)"
        );
        // Return cached result if available
        if let Some(cached) = self.is_traversible_cache.get() {
            return cached;
        }

        // Compute and cache the result
        let is_traversible = self
            .get_stat()
            .is_ok_and(|entry| FileType::from_stat(&entry) == FileType::Directory);

        self.is_traversible_cache.set(Some(is_traversible));
        is_traversible
    }

    /** Checks if the file is hidden (e.g., `.gitignore`, `.config`).

    A file is considered hidden if its filename (not the full path)
    starts with a dot ('.') character.

    Useful for filtering out hidden files in directory listings.

    # Examples

    ```
    use fdf::fs::DirEntry;
    use std::fs::File;
    use std::io::Write;

    // Create a temporary hidden file
    let tmp = std::env::temp_dir().join(".hidden_file.txt");
    File::create(&tmp).unwrap().write_all(b"content").unwrap();

    // Build DirEntry
    let entry = DirEntry::new(&tmp).unwrap();

    // Check if it's hidden
    assert!(entry.is_hidden());

    std::fs::remove_file(tmp).unwrap();
    ```

    ```
    use fdf::fs::DirEntry;
    use std::fs::File;

    // Regular non-hidden file
    let tmp = std::env::temp_dir().join("visible_file.txt");
    File::create(&tmp).unwrap();

    let entry = DirEntry::new(&tmp).unwrap();
    assert!(!entry.is_hidden());

    std::fs::remove_file(tmp).unwrap();
    ```

    ```
    use fdf::fs::DirEntry;
    use std::fs::File;

    // File with multiple dots but not hidden
    let tmp = std::env::temp_dir().join("backup.old.txt");
    File::create(&tmp).unwrap();

    let entry = DirEntry::new(&tmp).unwrap();
    assert!(!entry.is_hidden());

    std::fs::remove_file(tmp).unwrap();
    ```

    ```
    use fdf::fs::DirEntry;
    use std::fs::File;

    // Edge case: just a dot
    let tmp = std::env::temp_dir().join(".helo");
    File::create(&tmp).unwrap();

    let entry = DirEntry::new(&tmp).unwrap();
    assert!(entry.is_hidden());

    std::fs::remove_file(tmp).unwrap();
    ```
    */
    #[inline]
    #[must_use]
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "stylistic")]
    pub const fn is_hidden(&self) -> bool {
        // SAFETY: file_name_index() is guaranteed to be within bounds
        // and we're using pointer arithmetic which is const-compatible (slight const hack)
        unsafe { *self.as_ptr().cast::<u8>().add(self.file_name_index()) == b'.' }
    }

    /**
    Returns the directory name of the file (as bytes)
    ```
    use fdf::fs::DirEntry;
    use std::fs::File;
    use std::io::Write;

    // Test 1: Regular file path with directory structure
    let tmp = std::env::temp_dir().join("test_dir/file.txt");
    if let Some(parent) = tmp.parent() {
       std::fs::create_dir_all(parent).unwrap();
       }
       File::create(&tmp).unwrap();

       let entry = DirEntry::new(&tmp).unwrap();
       assert_eq!(entry.dirname(), b"test_dir");

       std::fs::remove_file(&tmp).unwrap();
       std::fs::remove_dir_all(tmp.parent().unwrap()).unwrap();


       let root_dir=DirEntry::new("/");
       assert!(root_dir.is_err() || root_dir.is_ok_and(|x| x.dirname()==b"/"));

       ```
       */
    #[inline]
    #[must_use]
    pub fn dirname(&self) -> &[u8] {
        debug_assert!(
            self.file_name_index() <= self.len(),
            "Indexing should always be within bounds"
        );

        if self.as_bytes() == b"/" {
            return b"/";
        }

        // SAFETY: the index is below the length of the path trivially
        unsafe {
            self //this is why we store the baseline, to check this and is hidden as above, its very useful and cheap
                .get_unchecked(..self.file_name_index().saturating_sub(1))
                .rsplit(|&b| b == b'/')
                .next()
                .unwrap_or(self.as_bytes())
        }
    }

    /**
      Extracts the extension of the file name, if any.

     The extension is defined as the substring after the last dot (`.`) character
     in the file name, excluding cases where the dot is the final character.

     # Examples

     ```
     use fdf::fs::DirEntry;
     use std::env::temp_dir;

     let tmp=std::env::temp_dir();
     let file_test=tmp.join("file.txt");
      std::fs::File::create(&file_test).unwrap();

     let path = DirEntry::new(&file_test).unwrap();
     assert_eq!(path.extension(), Some(b"txt".as_ref()));
      std::fs::remove_file(&file_test).unwrap();

     let root_dir=DirEntry::new("/");
     assert!(root_dir.is_err() || root_dir.is_ok_and(|path| path.extension().is_none()));
     ```
    */
    #[inline]
    pub fn extension(&self) -> Option<&[u8]> {
        let filename = self.file_name();
        let len = filename.len();

        if len <= 1 {
            return None;
        }

        // Search for the last dot in the filename, excluding the last character ('.''s dont count if they're the final character)
        // SAFETY: len is guaranteed within bounds
        let search_range = unsafe { &filename.get_unchecked(..len.saturating_sub(1)) };

        crate::util::memrchr(b'.', search_range).map(|pos| {
            // SAFETY:
            // - `pos` comes from `memrchr` which searches within `search_range`
            // - `search_range` is a subslice of `filename` (specifically `filename[..len-1]`)
            // - Therefore `pos` is a valid index in `filename`
            // - `pos + 1` is guaranteed to be ≤ len-1, so `pos + 1..` is a valid range
            unsafe { filename.get_unchecked(pos + 1..) }
        })
    }

    /**
    Creates a new [`DirEntry`] from the given path.

    This constructor attempts to resolve metadata for the provided path using
    a `lstat` call. If successful, it initialises a `DirEntry` with the path,
    file type, inode, and some derived metadata such as depth and file name index.



    # Examples
    ```
    use fdf::fs::DirEntry;
    use std::env;

    # fn main() -> Result<(), fdf::DirEntryError> {
       // Use the system temporary directory
       let tmp = env::temp_dir();

       // Create a DirEntry from the temporary directory path
       let entry = DirEntry::new(tmp)?;

       println!("inode: {}", entry.ino());
       Ok(())
       }
    ```



    # Errors

    The following returns an `DirEntryError::IOError` error when the file doesn't exist:

    ```
    use fdf::{fs::DirEntry, DirEntryError};
    use std::fs;

    // Verify the path doesn't exist first
    let nonexistent_path = "/definitely/not/a/real/file/lalalalalalalalalalalal";
    assert!(!fs::metadata(nonexistent_path).is_ok());

       // This will return an DirEntry::IOError because the file does not exist
    let result = DirEntry::new(nonexistent_path);
    match result {
           Ok(_) => panic!("this should never happen!"),
           Err(DirEntryError::IOError(_)) => {}, // Expected error
           Err(_) => panic!("Expected  error, got different error"),
           }

    ```
    */
    #[inline]
    pub fn new<T: AsRef<OsStr>>(path: T) -> Result<Self> {
        // It doesn't really matter if this constructor is 'expensive' mostly because the iterator constructs
        // this without lstat.
        let mut path_ref = path.as_ref().as_bytes();
        // Strip trailing slash

        if path_ref != b"/"
            && let Some(stripped) = path_ref.strip_suffix(b"/")
        {
            path_ref = stripped;
        }

        let cstring = std::ffi::CString::new(path_ref).map_err(DirEntryError::NulError)?;

        // extract information from successful stat
        let get_stat = stat_syscall!(lstat, cstring.as_ptr()).map_err(DirEntryError::IOError)?;
        let inode = access_stat!(get_stat, st_ino);
        let file_name_index = path_ref.file_name_index();
        let file_type = FileType::from_stat(&get_stat);
        Ok(Self {
            path: cstring.into(),
            file_type,
            inode,
            depth: 0,
            file_name_index,
            is_traversible_cache: Cell::new(None), //no need to check(we'd need to call stat instead!)
        })
    }

    /**
      Returns the last modification time of the file in UTC.

     This method performs an `lstat` system call to retrieve metadata for the entry.
     Unlike `stat`, it does **not** follow symbolic links. If the entry is a symlink,
     the modification time of the link itself is returned rather than that of its target.

     # Errors

     Returns an [`Err`] if:
     - The `lstat` call fails (for example, due to insufficient permissions or an invalid path).
     - The timestamp retrieved from the OS cannot be represented as a valid [`DateTime<Utc>`].

     # Notes

     The timestamp is constructed using seconds (`st_mtime`) and nanoseconds (`st_mtimensec`)
     from the underlying `stat` structure. These values are cast to `u32` internally
     to match the expected type for [`chrono::DateTime::from_timestamp`].

     # Examples

     ```no_run
     use fdf::fs::DirEntry;
     use chrono::Utc;

     let entry = DirEntry::new("/tmp/example.txt").unwrap();
     let modified = entry.modified_time().unwrap();

     println!("Last modified at: {}", modified.with_timezone(&Utc));
     ```
    */
    #[inline]
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "needs to be in u32 for chrono"
    )]
    pub fn modified_time(&self) -> Result<DateTime<Utc>> {
        let statted = self.get_lstat()?;

        DateTime::from_timestamp(
            access_stat!(statted, st_mtime),
            access_stat!(statted, st_mtimensec),
        )
        .ok_or(DirEntryError::TimeError)
    }

    /**
    Gets the file size in bytes.

    This method retrieves the file size by calling `get_lstat()` and extracting
    the `st_size` field from the stat structure. Unlike `get_stat()`, this
    method does not follow symbolic links - it returns the size of the symlink
    itself rather than the target file.

    # Errors

    Returns an error if:
    - The file doesn't exist
    - Permission is denied
    - The path is invalid
    - The lstat system call fails for any other reason

    # Returns

    The size of the file in bytes as an `u64`. For symbolic links, this returns
    the length of the symlink path itself, not the target file size.
    */
    #[inline]
    #[expect(clippy::cast_sign_loss, reason = "Size is a u64")]
    pub fn file_size(&self) -> Result<u64> {
        //https://github.com/rust-lang/rust/blob/bbb6f68e2888eea989337d558b47372ecf110e08/library/std/src/sys/fs/unix.rs#L442
        self.get_lstat().map(|s| s.st_size as _)
    }

    /**
     Returns an iterator over directory entries using the `readdir` API.

     This provides a higher-level, more portable interface for directory iteration
     compared to `getdents`. Suitable for most use cases where maximum performance
     isn't critical.

     # Errors

     Returns `Err` if:
     - The entry is not a directory
     - Permission restrictions prevent reading the directory
     - The directory has been removed or become inaccessible
     - Any other system error occurs during directory opening/reading

     # Examples

     ```
     use fdf::fs::DirEntry;
     use std::fs::{self, File};
     use std::io::Write;
     use std::sync::Arc;

     // Create a temporary directory with test files
     let temp_dir = std::env::temp_dir().join("test_readdir");
     fs::create_dir(&temp_dir).unwrap();

     // Create test files
     File::create(temp_dir.join("file1.txt")).unwrap().write_all(b"test").unwrap();
     File::create(temp_dir.join("file2.txt")).unwrap().write_all(b"test").unwrap();
     fs::create_dir(temp_dir.join("subdir")).unwrap();

     // Create DirEntry for the temporary directory
     let entry = DirEntry::new(&temp_dir).unwrap();

     // Use readdir to iterate through directory contents
     let mut entries: Vec<_> = entry.readdir().unwrap().collect();
     entries.sort_by_key(|e| e.file_name().to_vec());

     // Should contain 3 entries: 2 files and 1 directory
     assert_eq!(entries.len(), 3);
     assert!(entries.iter().any(|e| e.file_name() == b"file1.txt"));
     assert!(entries.iter().any(|e| e.file_name() == b"file2.txt"));
     assert!(entries.iter().any(|e| e.file_name() == b"subdir"));
     fs::remove_dir_all(&temp_dir).unwrap();
    ```
    */
    #[inline]
    pub fn readdir(&self) -> Result<ReadDir> {
        ReadDir::new(self)
    }

    /**
     Low-level directory iterator using the `getdents64` system call.

     This method provides high-performance directory scanning by using a large buffer
     (typically ~4.1KB) to minimise system calls. It's Linux-specific and generally
     faster than `readdir` for bulk directory operations.

     # Errors

     Returns `Err` if:
     - The entry is not a directory
     - Permission restrictions prevent reading the directory
     - The directory file descriptor cannot be opened
     - Buffer allocation fails
     - Any other system error occurs during the `getdents` operation

     # Platform Specificity

     This method is only available on Linux/Android targets due to its dependence on
     the `getdents64` system call.

     # Examples

     ```
     use fdf::fs::DirEntry;
     use std::fs::{self, File};
     use std::io::Write;

     // Create a temporary directory with test files
     let temp_dir = std::env::temp_dir().join("test_getdents");
     fs::create_dir(&temp_dir).unwrap();

     // Create test files
     File::create(temp_dir.join("file1.txt")).unwrap().write_all(b"test").unwrap();
     File::create(temp_dir.join("file2.txt")).unwrap().write_all(b"test").unwrap();
     fs::create_dir(temp_dir.join("subdir")).unwrap();

     // Create DirEntry for the temporary directory
     let entry= DirEntry::new(&temp_dir).unwrap();

     // Use getdents to iterate through directory contents
     let mut entries: Vec<_> = entry.getdents().unwrap().collect();
     entries.sort_by_key(|e| e.file_name().to_vec());

     // Should contain 3 entries: 2 files and 1 directory
     assert_eq!(entries.len(), 3);
     assert!(entries.iter().any(|e| e.file_name() == b"file1.txt"));
     assert!(entries.iter().any(|e| e.file_name() == b"file2.txt"));
     assert!(entries.iter().any(|e| e.file_name() == b"subdir"));

     // Clean up
     fs::remove_dir_all(&temp_dir).unwrap();
    ```
    */
    #[inline]
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub fn getdents(&self) -> Result<crate::fs::GetDents> {
        crate::fs::GetDents::new(self)
    }

    /**
    Low-level directory iterator using macOS `getdirentries` API.

    This method provides a macOS-specific, high-performance streaming iterator
    over directory entries by leveraging the platform's `getdirentries(2)`
    family. It is analogous to the Linux `getdents` specialisation but
    implemented for BSD-derived macOS interfaces and conventions.

    It implements a specialised EOF trick to avoid an extra syscall to terminate reading


    # Platform
    - macOS only

    # Errors
    Returns `Err` when the directory cannot be opened or read, when the
    target is not a directory, or when permissions prevent iteration.


    For examples: See documentation on `readdir`
    ```
    */
    #[inline]
    #[cfg(target_os = "macos")]
    pub fn getdirentries(&self) -> Result<crate::fs::iter::GetDirEntries> {
        crate::fs::iter::GetDirEntries::new(self)
    }
}
