/*!
 # fdf - A High-Performance Parallel File System Traversal Library

 `fdf` is a Rust library designed for efficient, parallel directory traversal
 with extensive filtering capabilities. It leverages Rayon for parallel processing
 and uses platform-specific optimisations for maximum performance.

 **This will be renamed before a 1.0!**

 ## Features

 - **Parallel Processing**: Utilises a custom work-stealing scheduler for concurrent
   directory traversal
 - **Platform Optimisations**: Linux/Android specific `getdents` system calls for optimal
   performance, with fallbacks for other platforms
 - **Flexible Filtering**: Support for multiple filtering criteria:
   - File name patterns (regex,glob)
   - File size ranges
   - File types (regular, directory, symlink, etc.)
   - File extensions
   - Hidden file handling
   - Custom filter functions
 - **Cycle Detection**: Automatic symlink cycle prevention using inode caching
 - **Depth Control**: Configurable maximum search depth
 - **Path Canonicalisation**: Optional path resolution to absolute paths
 - **Error Handling**: Configurable error reporting with detailed diagnostics

 ## Performance Characteristics

 - Uses mimalloc as global allocator on supported platforms for improved (disabled with --no-default-features)
   memory allocation performance
 - Batched result delivery to minimise channel contention
 - Zero-copy path handling where possible
 - Avoids unnecessary `stat` calls through careful API design

 ## Platform Support

 - **Linux/Android**: Optimised with direct `getdents64` system calls
 - **macOS**: Support for `__getdirentries64` (to be allowed to be disabled in a future update)
 - **Other Unix-like**: Fallback to standard library functions
 - **Windows**: Not currently supported (PRs welcome!)

 ## Quick Start

 ```rust
 use fdf::{walk::Finder, fs::DirEntry, SearchConfigError};
 use std::sync::mpsc::Receiver;

 fn find_files() -> Result<impl Iterator<Item = DirEntry>, SearchConfigError> {
    let finder = Finder::init("/path/to/search")
        .pattern("*.rs")
        .keep_hidden(false)
        .max_depth(Some(3))
        .follow_symlinks(true)
        .canonicalise_root(true) // Resolve the root to a full path
        .build()?;

    finder.traverse()
}
```

Setting custom filters example
```no_run

use fdf::{walk::Finder, filters::FileTypeFilter};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let finder = Finder::init("/var/log")
        .pattern("*.log")
        .keep_hidden(false)
        .type_filter(Some(FileTypeFilter::File))
        //set the custom filter
        .filter(Some(|entry| {
            entry.extension().is_some_and( |ext| ext.eq_ignore_ascii_case(b"log"))
        }))
        .build()?;

    let entries = finder.traverse()?;
    let mut count = 0;

    for entry in entries {
        count += 1;
        println!("Matched: {}", entry.as_path().display());
    }

    println!("Found {count} log files");
    Ok(())
}
```
*/

#[cfg(target_pointer_width = "32")]
compile_error!("Not supported on 32bit, I may do if a PR is sent!");

#[cfg(target_os = "windows")]
compile_error!("This application is not supported on Windows (yet)");

// Re-exports
pub use chrono;
pub use libc;

#[macro_use]
pub(crate) mod macros;

#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub(crate) use libc::{dirent as dirent64, readdir as readdir64};

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) use libc::{dirent64, readdir64};

pub use std::os::raw::c_char;

mod test;

mod error;
pub use error::{DirEntryError, FilesystemIOError, SearchConfigError, TraversalError};

mod config;
pub use config::SearchConfig;
pub mod filters;
pub mod fs;
pub mod util;
pub mod walk;
