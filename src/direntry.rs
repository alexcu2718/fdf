//! A high-performance, parallel directory traversal and file search library.
//!
//! This library provides efficient file system traversal with features including:
//! - Parallel directory processing using Rayon
//! - Low-level system calls for optimal performance on supported platforms
//! - Flexible filtering by name, size, type, and custom criteria
//! - Symbolic link handling with cycle detection
//! - Cross-platform support with platform-specific optimisations
//!
//! # Examples
//! Simple file search example
//! ```no_run
//! use fdf::{Finder, SearchConfig};
//! use std::sync::Arc;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let finder= Finder::init("/some/path").pattern("*.txt")
//!         .build()
//!         .expect("Failed to build finder");
//!
//!     let receiver = finder.traverse()
//!         .expect("Failed to start traversal");
//!
//!     let mut file_count = 0;
//!     let mut batch_count = 0;
//!
//!     for batch in receiver {
//!         batch_count += 1;
//!         for entry in batch {
//!             file_count += 1;
//!             println!("Found: {:?}", entry);
//!         }
//!     }
//!
//!     println!("Discovered {} files in {} batches", file_count, batch_count);
//!
//!     Ok(())
//! }
//! ```
//!
//! Advanced file search example
//! ```no_run
//! use fdf::{Finder, SizeFilter, FileTypeFilter};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let finder = Finder::init("/some/path").pattern("*.txt")
//!         .keep_hidden(false)
//!         .case_insensitive(true)
//!         .keep_dirs(true)
//!         .max_depth(Some(5))
//!         .follow_symlinks(false)
//!         .filter_by_size(Some(SizeFilter::Min(1024)))
//!         .type_filter(Some(FileTypeFilter::File))
//!         .show_errors(true)
//!         .build()
//!         .map_err(|e| format!("Failed to build finder: {}", e))?;
//!
//!     let receiver = finder.traverse()
//!         .map_err(|e| format!("Failed to start traversal: {}", e))?;
//!
//!     let mut file_count = 0;
//!
//!     for batch in receiver {
//!         for entry in batch {
//!             file_count += 1;
//!             println!("{:?}  )",
//!                 entry,
//!             
//!             );
//!         }
//!     }
//!
//!     println!("Search completed! Found {} files", file_count);
//!
//!     Ok(())
//! }

use crate::{BytePath as _, DirEntryError, FileDes, ReadDir, Result, filetype::FileType};
use core::cell::Cell;
use core::ptr::NonNull;
use libc::{
    AT_SYMLINK_FOLLOW, AT_SYMLINK_NOFOLLOW, F_OK, O_CLOEXEC, O_DIRECTORY, O_NONBLOCK, R_OK, W_OK,
    X_OK, access, c_char, fstatat, lstat, open, opendir, realpath, stat,
};
use std::{
    ffi::{CStr, OsStr},
    os::unix::ffi::OsStrExt as _,
};
/**
  A struct representing a directory entry with minimal memory overhead.

  This struct is designed for high-performance file system traversal and analysis.
  It holds metadata and a path to a file or directory, optimised for size
  and efficient access to its components.

  The struct's memory footprint is
  - **Path**: 16 bytes, Box<CStr>, retaining compatibility for use in libc but converting to &[u8] trivially(as deref)
  - **File type**: A 1-byte enum representing the entry's type (file, directory, etc.).
  - **Inode**: An 8-byte integer for the file's unique inode number.
  - **Depth**: A 2-byte integer indicating the entry's depth from the root.
  - **File name index**: A 2-byte integer pointing to the start of the file name within the path buffer.

  # Examples

  ```
  use fdf::DirEntry;
  use std::path::Path;
  use std::fs::File;
  use std::io::Write;
  use std::sync::Arc;



  // Create a temporary directory for the test
  let temp_dir = std::env::temp_dir();

  let file_path = temp_dir.join("test_file.txt");

  // Create a file inside the temporary directory
  {
      let mut file = File::create(&file_path).expect("Failed to create file");
      writeln!(file, "Hello, world!").expect("Failed to write to file");
  }

  // Create a DirEntry from the temporary file path
   let entry = DirEntry::new(&file_path).unwrap();
  assert!(entry.is_regular_file());
  assert_eq!(entry.file_name(), b"test_file.txt");


  ```
*/
#[derive(Clone)] //could probably implement a more specialised clone.
pub struct DirEntry {
    /// Path to the entry, stored as a Boxed `CStr`
    //(to avoid storing the capacity)
    /// This allows easy C ffi by just calling `.as_cstr().as_ptr()`
    pub(crate) path: Box<CStr>, //16 bytes

    /// File type (file, directory, symlink, etc.).
    ///
    /// Stored as a 1-byte enum.
    pub(crate) file_type: FileType,

    /// Inode number of the file.
    pub(crate) inode: u64, //8 bytes
    //
    /// Depth of the directory entry relative to the root.
    ///
    pub(crate) depth: u16, //2bytes

    /// Offset in the path buffer where the file name starts.
    ///
    /// This helps quickly extract the file name from the full path.
    pub(crate) file_name_index: u16, //2bytes
    ///
    /// `None` means not computed yet, `Some(bool)` means cached result.
    pub(crate) is_traversible_cache: Cell<Option<bool>>, //1byte
                                                         //30 bytes, rounded to 32
                                                         // We could add an extra bool on it? and still abuse null pointer optimisation for Options
}

impl core::ops::Deref for DirEntry {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_bytes()
    }
}

// Also implement AsRef<[u8]> for convenience
impl AsRef<[u8]> for DirEntry {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl DirEntry {
    /**
    Checks if the entry is an executable file.

     This is a **costly** operation as it performs an `access` system call.

    # Examples

    ```
      use fdf::DirEntry;
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
    pub fn is_executable(&self) -> bool {
        // X_OK is the execute permission, requires access call
        // SAFETY: We know the path is valid because internally it's a cstr
        self.is_regular_file() && unsafe { access(self.path.as_ptr(), X_OK) == 0 }
    }

    /*
     Returns a raw pointer to the underlying C string.

     This provides access to the null-terminated C string representation
     of the file path for use with FFI functions.

     # Returns
     A raw pointer to the null-terminated C string.

    */
    #[inline]
    pub const fn as_ptr(&self) -> *const c_char {
        self.path.as_ptr()
    }

    /// Cost free check for block devices
    #[inline]
    #[must_use]
    pub const fn is_block_device(&self) -> bool {
        self.file_type.is_block_device()
    }

    #[inline]
    /**
     * Opens the directory and returns a file descriptor.

    This is a low-level operation that opens the directory with the following flags:
     - `O_CLOEXEC`: Close the file descriptor on exec
     - `O_DIRECTORY`: Fail if not a directory
    - `O_NONBLOCK`: Open in non-blocking mode

     # Errors

     Returns an error if:
    - The directory doesn't exist or can't be opened
    - The path doesn't point to a directory
    - Permission is denied
    */
    pub fn open_fd(&self) -> Result<i32> {
        // Opens the file and returns a file descriptor.
        // This is a low-level operation that may fail if the file does not exist or cannot be opened.
        const FLAGS: i32 = O_CLOEXEC | O_DIRECTORY | O_NONBLOCK;

        //   #[cfg(target_os="linux")]
        //  let fd=unsafe{crate::syscalls::open_asm(self.path.as_ref(),FLAGS)};
        // #[cfg(not(target_os="linux"))]
        // SAFETY: the pointer is null terminated
        let fd = unsafe { open(self.as_ptr(), FLAGS) };

        if fd < 0 {
            Err(std::io::Error::last_os_error().into())
        } else {
            Ok(fd)
        }
    }

    #[inline]
    /**  Opens a directory stream for reading directory entries.

     This function returns a `NonNull<libc::DIR>` pointer to the directory stream,
     which can be used with `readdir` to iterate over directory entries.


     # Errors

     Returns an error if:
     - The directory doesn't exist or can't be opened
     - The path doesn't point to a directory
     - Permission is denied
     - System resources are exhausted
    */
    pub fn open_dir(&self) -> Result<NonNull<libc::DIR>> {
        // SAFETY: we are passing a null terminated directory to opendir

        let dir = unsafe { opendir(self.as_ptr()) };
        // This function reads the directory entries and populates the iterator.
        // It is called when the iterator is created or when it needs to be reset.
        if dir.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: know it's non-null
        Ok(unsafe { NonNull::new_unchecked(dir) }) // Return a pointer to the start `DIR` stream
    }
    #[inline]
    #[must_use]
    // Converts to a lossy string for ease of use
    pub fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(self)
    }

    #[inline]
    /**
    Returns the underlying bytes as a UTF-8 string slice if valid.
    # Errors
    Returns `Err` if the bytes are not valid UTF-8.
    */
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

    ///Cost free check for directories
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
    #[inline]
    #[must_use]
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "We're only matching on relevant types"
    )]
    /**
        Checks if the entry is empty.

        For files, it checks if the size is zero. For directories, it checks if there are no entries.
        This is a **costly** operation as it requires system calls (`stat` or `getdents`/`readdir`).

        # Examples

        ```
        use fdf::DirEntry;
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
    pub fn is_empty(&self) -> bool {
        match self.file_type() {
            FileType::RegularFile => self.file_size().is_ok_and(|size| size == 0),
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

    /**
    Returns the full path of this directory entry as a `CStr`.

    This function provides direct access to the underlying null-terminated C string
    representing the entry’s absolute or relative path (depending on how it was created).

     The returned reference is valid for the lifetime of the `DirEntry` and does not allocate.

    # Examples

     ```
     use fdf::DirEntry;
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
     use fdf::DirEntry;
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
    use fdf::DirEntry;
    use std::fs::File;

     // File name with spaces
    let tmp = std::env::temp_dir().join("some name.txt");
    File::create(&tmp).unwrap();

    let entry = DirEntry::new(&tmp).unwrap();
    let name = entry.file_name_cstr();

    assert_eq!(name.to_str().unwrap(), "some name.txt");

    std::fs::remove_file(tmp).unwrap();
    ```
    */
    pub fn file_name_cstr(&self) -> &CStr {
        let bytes = self.path.to_bytes_with_nul();
        // SAFETY:
        // - `file_name_index()` points to the start of the file name within `bytes`.
        // - The slice from this index to the end includes the null terminator.
        // - The slice is guaranteed to represent a valid C string.
        // We transmute the slice into a `&CStr` reference for zero-copy access.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        #[expect(
            clippy::transmute_ptr_to_ptr,
            reason = "They have the same representation due to repr transparent on cstr"
        )]
        unsafe {
            core::mem::transmute(bytes.get_unchecked(self.file_name_index()..))
        }
    }

    #[inline]
    #[must_use]
    // Returns the name of the file (as bytes, no null terminator)
    pub fn file_name(&self) -> &[u8] {
        debug_assert!(
            self.len() >= self.file_name_index(),
            "this should always be equal or below (equal only when root)"
        );
        // SAFETY: the index is below the length of the path trivially
        unsafe { self.get_unchecked(self.file_name_index()..) }
    }

    /// Takes the value of the path and gives the raw representation
    #[inline]
    pub fn as_inner(self) -> Box<CStr> {
        self.path
    }

    #[inline]
    #[allow(clippy::multiple_unsafe_ops_per_block)] //annoying
    /// Private function for complicated reasons
    pub(crate) fn get_realpath(&self) -> Result<Box<CStr>> {
        // SAFETY: Guaranteed null terminated due to underlying CStr representation

        let ptr = unsafe { realpath(self.as_ptr(), core::ptr::null_mut()) }; //realpath implicitly mallocs, hence need to free.

        if ptr.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }

        // SAFETY: We know the path is valid because it's a  guaranteed null terminated+non null
        let boxed = unsafe { Box::from(CStr::from_ptr(ptr)) };
        // SAFETY: the pointer points to valid malloc'ed memory(we have checked for null), it is safe to free it now
        unsafe { libc::free(ptr.cast()) } //see definition below to check std library implementation
        //free the pointer to stop leaking
        // I wonder if you can do Box::from_raw on the above, I believe rust's allocator would be the libc's version of malloc
        // however because I am using a custom allocator `MiMalloc` I'm not sure if how they'd interact,
        // however i've never introspected memory pages until not except primitive methods
        // however this would be a very amusing micro optimisation, it's too much effort, i'll read into it more when i feel like
        Ok(boxed)
    }

    /* (I spent a lot of time debating this function!)
    https://man7.org/linux/man-pages/man3/realpath.3.html
    DESCRIPTION         top

       realpath() expands all symbolic links and resolves references to
       /./, /../ and extra '/' characters in the null-terminated string
       named by path to produce a canonicalized absolute pathname.  The
       resulting pathname is stored as a null-terminated string, up to a
       maximum of PATH_MAX bytes, in the buffer pointed to by
       resolved_path.  The resulting path will have no symbolic link, /./
       or /../ components.

       If resolved_path is specified as NULL, then realpath() uses
       malloc(3) to allocate a buffer of up to PATH_MAX bytes to hold the
       resolved pathname, and returns a pointer to this buffer.  The
       caller should deallocate this buffer using free(3).



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
    /// Converts a directory entry to a full, canonical path, resolving all symlinks
    ///
    /// This is a **costly** operation as it involves a system call (`realpath`).
    /// If the filetype is a symlink, it invokes a stat call to find the realtype, otherwise stat is not called.
    ///
    /// # Errors
    ///
    /// Returns an `Err`
    ///
    /// It can be one of the following,
    ///  `FileType::AccessDenied`, (EACCESS)
    ///  `FileType::TooManySymbolicLinks`, ( ELOOP)
    ///  `FileType::InvalidPath` (ENOENT)
    /// (There may be more, this documentation is not complete) TODO!
    ///
    ///    
    ///
    /// # Examples
    /// these tests are broken on macos because of funky stuff  with mac's privacy/security settings.
    /// ```ignore
    ///
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
    /// let entry = DirEntry::new(&symlink_path).unwrap();
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
    ///
    /// ```
    pub fn to_full_path(&self) -> Result<Self> {
        let full_path = self.get_realpath()?;

        let file_name_index = full_path.to_bytes().file_name_index(); //used for indexing.
        // Computing result here to avoid borrow issues

        let (file_type, ino) = if self.is_symlink() {
            let statted = self.get_stat()?;
            //if it's a symlink, we need to resolve it.
            (FileType::from_stat(&statted), access_stat!(statted, st_ino))
        } else {
            (self.file_type(), self.ino()) //ino will not change if it's not a symlink, neither will file type!
        };

        let boxed = Self {
            path: full_path,
            file_type,
            inode: ino,
            depth: self.depth, //inherit depth, may need to revisit this
            file_name_index,
            is_traversible_cache: Cell::new(Some(file_type == FileType::Directory)), //we can check it's traversibility directly here because of it being resolved
        };

        Ok(boxed)
    }

    #[inline]
    /**
    Checks if the file or directory is readable by the current process.

    This uses the `access` system call with `R_OK` to check read permissions
    without actually opening the file. It follows symlinks.

    # Returns

    `true` if the current process has read permission, `false` otherwise.

    `false` if the file doesn't exist or on permission errors.
    */
    pub fn is_readable(&self) -> bool {
        // SAFETY: The path is guaranteed to be a be null terminated
        unsafe { access(self.as_ptr(), R_OK) == 0 }
    }

    #[inline]
    /**
    Checks if the file or directory is writable by the current process.

    This uses the `access` system call with `W_OK` to check write permissions
    without actually opening the file. It follows symlinks.

    Note: This may perform unnecessary syscalls for obviously unwritable paths
    like system directories. Future optimizations could exclude certain paths.

    # Returns

    `true` if the current process has write permission, `false` otherwise.
    `false` if the file doesn't exist or on permission errors.
    */
    pub fn is_writable(&self) -> bool {
        //maybe i can automatically exclude certain files from this check to
        //then reduce my syscall total, would need to read into some documentation. zadrot ebaniy
        // SAFETY: The path is guaranteed to be a null terminated
        unsafe { access(self.as_ptr(), W_OK) == 0 }
    }

    #[inline]
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

    # Returns

    A `stat` structure containing file metadata on success.
    */
    pub fn get_lstat(&self) -> Result<stat> {
        Self::get_lstat_private(self.as_ptr())
    }

    #[inline]
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

    # Returns

    A `stat` structure containing file metadata on success.
    */
    pub fn get_stat(&self) -> Result<stat> {
        // Simple wrapper to avoid code duplication so I can use the private method within the crate
        Self::get_stat_private(self.as_ptr())
    }

    #[inline]
    /**
     * Checks if the file exists.
     *
     * This makes a system call to check file existence.
     */
    pub fn exists(&self) -> bool {
        // SAFETY: The path is guaranteed to be null terminated
        unsafe { access(self.as_ptr(), F_OK) == 0 }
    }

    #[inline]
    /**
     * Gets file metadata using lstatat for a file relative to a directory file descriptor.
     *
     * This function uses `fstatat` with `AT_SYMLINK_NOFOLLOW` to get metadata without
     * following symbolic links, similar to `lstat` but relative to a directory fd.
     *
     * # Arguments
     * * `fd` - Directory file descriptor to use as the base for relative path resolution
     *
     * # Returns
     * A `stat` structure containing file metadata on success.
     *
     * # Errors
     * Returns `DirEntryError::InvalidStat` if the stat operation fails
     */
    pub fn get_lstatat(&self, fd: &FileDes) -> Result<stat> {
        let mut stat_buf = core::mem::MaybeUninit::<stat>::uninit();
        // SAFETY:
        // - The path is guaranteed to be null-terminated (CStr)
        // - fd must be a valid file descriptor
        // - stat_buf is properly allocated
        let res = unsafe {
            fstatat(
                fd.0,
                self.as_ptr(),
                stat_buf.as_mut_ptr(),
                AT_SYMLINK_NOFOLLOW,
            )
        };

        if res == 0 {
            // SAFETY: If the return code is 0, we know the stat structure has been properly initialized
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }

    #[inline]
    /**
     * Gets file metadata using statat for a file relative to a directory file descriptor.
     *
     * This function uses `fstatat` with `AT_SYMLINK_FOLLOW` to get metadata by
     * following symbolic links, similar to `stat` but relative to a directory fd.
     *
     * # Arguments
     * * `fd` - Directory file descriptor to use as the base for relative path resolution
     *
     * # Returns
     * A `stat` structure containing file metadata on success.
     *
     * # Errors
     * Returns `DirEntryError::InvalidStat` if the stat operation fails
     */
    pub fn get_statat(&self, fd: &FileDes) -> Result<stat> {
        let mut stat_buf = core::mem::MaybeUninit::<stat>::uninit();
        // SAFETY:
        // - The path is guaranteed to be null-terminated (CStr)
        // - fd must be a valid file descriptor
        // - stat_buf is properly allocated
        let res = unsafe {
            fstatat(
                fd.0,
                self.as_ptr(),
                stat_buf.as_mut_ptr(),
                AT_SYMLINK_FOLLOW,
            )
        };

        if res == 0 {
            // SAFETY: If the return code is 0, we know the stat structure has been properly initialized
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }

    #[inline]
    pub(crate) fn get_lstat_private(ptr: *const c_char) -> Result<stat> {
        let mut stat_buf = core::mem::MaybeUninit::<stat>::uninit();
        // SAFETY: We know the path is valid because internally it's a cstr
        let res = unsafe { lstat(ptr, stat_buf.as_mut_ptr()) };

        if res == 0 {
            // SAFETY: If the return code is 0, we know it's been initialised properly
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }

    #[inline]
    pub(crate) fn get_stat_private(ptr: *const c_char) -> Result<stat> {
        let mut stat_buf = core::mem::MaybeUninit::<stat>::uninit();
        // SAFETY: We know the path is valid because internally it's a cstr
        let res = unsafe { stat(ptr, stat_buf.as_mut_ptr()) };

        if res == 0 {
            // SAFETY: If the return code is 0, we know it's been initialised properly
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(crate::DirEntryError::InvalidStat)
        }
    }

    #[inline]
    #[must_use]
    /// Cost free conversion to bytes (because it is already is bytes)
    pub const fn as_bytes(&self) -> &[u8] {
        // SAFETY: Avoid UB check, it's guaranteed to be in range due to having 1 less than the 'true' len
        // and guaranteed non null
        unsafe {
            &*core::ptr::slice_from_raw_parts(self.path.to_bytes_with_nul().as_ptr(), self.len())
            // len is the length of the non-null terminated internal buffer.
        }
    }

    #[inline]
    /**
    Returns the length of the path string in bytes.

    The length excludes the internal null terminator, so it matches
    what you'd expect from a regular Rust string slice length.

    # Examples
    ```
    use fdf::DirEntry;
    use std::fs::File;

    let tmp = std::env::temp_dir().join("test_file.txt");
    File::create(&tmp).unwrap();

    let entry = DirEntry::new(&tmp).unwrap();
    // Length matches the path string without null terminator
    assert_eq!(entry.len(), tmp.as_os_str().len());

    std::fs::remove_file(tmp).unwrap();
    ```
    */
    pub const fn len(&self) -> usize {
        self.path.count_bytes()
    }

    #[inline]
    #[must_use]
    /// Returns the file type of the file (eg directory, regular file, etc)
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
    ///returns the inode number of the file, cost free check
    ///
    ///
    /// this is a unique identifier for the file on the filesystem, it is not the same
    /// as the file name or path, it is a number that identifies the file on the
    /// It should be u32 on BSD's but I use u64 for consistency across platforms
    pub const fn ino(&self) -> u64 {
        self.inode
    }

    #[inline]
    #[must_use]
    /// Applies a filter condition
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
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This is exhaustive because only checking traversible types"
    )]
    /// Checks if the file is a directory or symlink (but internally a directory)
    pub fn is_traversible(&self) -> bool {
        match self.file_type {
            FileType::Directory => true,
            FileType::Symlink => self.check_symlink_traversibility(),
            _ => false,
        }
    }

    /// Checks if a symlink points to a traversible directory, caching the result.
    #[inline]
    pub(crate) fn check_symlink_traversibility(&self) -> bool {
        // Return cached result if available
        debug_assert!(
            self.file_type() == FileType::Symlink,
            "we only expect symlinks to use this function(hence private)"
        );
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

    #[inline]
    #[must_use]
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "stylistic")]
    #[expect(
        clippy::cast_sign_loss,
        reason = "casting i8 to u8 is fine for characters"
    )]
    /** Checks if the file is hidden (e.g., `.gitignore`, `.config`).

    A file is considered hidden if its filename (not the full path)
    starts with a dot ('.') character.

    Useful for filtering out hidden files in directory listings.

    # Examples

    ```
    use fdf::DirEntry;
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
    use fdf::DirEntry;
    use std::fs::File;

    // Regular non-hidden file
    let tmp = std::env::temp_dir().join("visible_file.txt");
    File::create(&tmp).unwrap();

    let entry = DirEntry::new(&tmp).unwrap();
    assert!(!entry.is_hidden());

    std::fs::remove_file(tmp).unwrap();
    ```

    ```
    use fdf::DirEntry;
    use std::fs::File;

    // File with multiple dots but not hidden
    let tmp = std::env::temp_dir().join("backup.old.txt");
    File::create(&tmp).unwrap();

    let entry = DirEntry::new(&tmp).unwrap();
    assert!(!entry.is_hidden());

    std::fs::remove_file(tmp).unwrap();
    ```

    ```
    use fdf::DirEntry;
    use std::fs::File;

    // Edge case: just a dot
    let tmp = std::env::temp_dir().join(".helo");
    File::create(&tmp).unwrap();

    let entry = DirEntry::new(&tmp).unwrap();
    assert!(entry.is_hidden());

    std::fs::remove_file(tmp).unwrap();
    ```
    */
    pub const fn is_hidden(&self) -> bool {
        // SAFETY: file_name_index() is guaranteed to be within bounds
        // and we're using pointer arithmetic which is const-compatible (slight const hack)
        unsafe { *self.as_cstr().as_ptr().add(self.file_name_index()) as u8 == b'.' }
    }
    #[inline]
    #[must_use]
    /// Returns the directory name of the file (as bytes)
    pub fn dirname(&self) -> &[u8] {
        debug_assert!(
            self.file_name_index() <= self.len(),
            "Indexing should always be within bounds"
        );
        // SAFETY: the index is below the length of the path trivially
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
    /// Returns the parent directory of the file (as bytes)
    pub fn parent(&self) -> &[u8] {
        debug_assert!(
            self.file_name_index() <= self.len(),
            "Indexing should always be within bounds"
        );
        // SAFETY: the index is below the length of the path trivially
        unsafe { self.get_unchecked(..core::cmp::max(self.file_name_index() - 1, 1)) }
    }

    #[inline]
    /// Creates a new [`DirEntry`] from the given path.
    ///
    /// This constructor attempts to resolve metadata for the provided path using
    /// a `lstat` call. If successful, it initialises a `DirEntry` with the path,
    /// file type, inode, and some derived metadata such as depth and file name index.
    ///
    ///
    /// - `DirEntryError::InvalidStat` if the underlying `lstat` call fails. (aka permissions/file doesnt exist)
    ///
    /// # Examples
    /// ```
    /// use fdf::DirEntry;
    /// use std::env;
    ///
    /// # fn main() -> Result<(), fdf::DirEntryError> {
    /// // Use the system temporary directory
    /// let tmp = env::temp_dir();
    ///
    /// // Create a DirEntry from the temporary directory path
    /// let entry = DirEntry::new(tmp)?;
    ///
    /// println!("inode: {}", entry.ino());
    ///  Ok(())
    /// }
    /// ```
    ///
    ///
    ///
    /// # Errors
    ///
    /// The following returns an `InvalidStat` error when the file doesn't exist:
    ///
    /// ```
    /// use fdf::{DirEntry, DirEntryError};
    /// use std::fs;
    ///
    /// // Verify the path doesn't exist first
    /// let nonexistent_path = "/definitely/not/a/real/file/lalalalalalalalalalalal";
    /// assert!(!fs::metadata(nonexistent_path).is_ok());
    ///
    /// // This will return an InvalidStat error because the file does not exist
    /// let result = DirEntry::new(nonexistent_path);
    /// match result {
    ///     Ok(_) => panic!("Expected InvalidStat error"),
    ///     Err(DirEntryError::InvalidStat) => {}, // Expected error
    ///     Err(_) => panic!("Expected InvalidStat error, got different error"),
    /// }
    /// # // The test passes if we reach this point without panicking
    /// # Ok::<(), DirEntryError>(())
    /// ```
    pub fn new<T: AsRef<OsStr>>(path: T) -> Result<Self> {
        let path_ref = path.as_ref().as_bytes(); //TODO GET RID OF UNWRAP HERE
        #[allow(clippy::map_err_ignore)] //lazy(don't want to also increase size of enum)
        let cstring = std::ffi::CString::new(path_ref).map_err(|_| DirEntryError::NullError)?;

        // extract information from successful stat
        let get_stat = Self::get_lstat_private(cstring.as_ptr())?;
        let inode = access_stat!(get_stat, st_ino);
        Ok(Self {
            path: cstring.into(),
            file_type: get_stat.into(),
            inode,
            depth: 0,
            file_name_index: path_ref.file_name_index(),
            is_traversible_cache: Cell::new(None), //no need to check
        })
    }

    #[inline]
    #[expect(clippy::cast_sign_loss, reason = "needs to be in u32 for chrono")]
    #[allow(clippy::missing_errors_doc)] //fixing errors later
    pub fn modified_time(&self) -> Result<chrono::DateTime<chrono::Utc>> {
        let statted = self.get_lstat()?;
        chrono::DateTime::from_timestamp(
            access_stat!(statted, st_mtime),
            access_stat!(statted, st_mtimensec),
        )
        .ok_or(DirEntryError::TimeError)
    }

    #[inline]
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

    The size of the file in bytes as an `i64`. For symbolic links, this returns
    the length of the symlink path itself, not the target file size.
    */
    pub fn file_size(&self) -> Result<i64> {
        self.get_lstat().map(|s| s.st_size as _)
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
    /// let entry = DirEntry::new(&temp_dir).unwrap();
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
    pub fn readdir(&self) -> Result<impl Iterator<Item = Self>> {
        ReadDir::new(self)
    }
    #[inline]
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
    /// let entry= DirEntry::new(&temp_dir).unwrap();
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
        use crate::iter::GetDents;
        GetDents::new(self)
    }
}
