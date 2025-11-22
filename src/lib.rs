/*!
 # fdf - A High-Performance Parallel File System Traversal Library

 `fdf` is a Rust library designed for efficient, parallel directory traversal
 with extensive filtering capabilities. It leverages Rayon for parallel processing
 and uses platform-specific optimisations for maximum performance.

 **This will be renamed before a 1.0!**

 ## Features

 - **Parallel Processing**: Utilises Rayon's work-stealing scheduler for concurrent
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

 - Uses mimalloc as global allocator on supported platforms for improved
   memory allocation performance
 - Batched result delivery to minimise channel contention
 - Zero-copy path handling where possible
 - Avoids unnecessary `stat` calls through careful API design
 - Makes up to 50% less `getdents` syscalls on linux/android, check the source code)

 ## Platform Support

 - **Linux/Android**: Optimised with direct `getdents` system calls
 - **BSD's/macOS**: Standard `readdir` with potential for future changes like `fts_open` (unlikely probably)
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

    println!("Found {} log files", count);
    Ok(())
}
```
*/
#[cfg(any(target_os = "vita", target_os = "hurd", target_os = "nuttx"))]
compile_error!(
    "This application is not supported on PlayStation Vita/hurd/nuttx, It may be if I'm ever bothered!"
);

#[cfg(target_pointer_width = "32")]
compile_error!("Not supported on 32bit, I may do if a PR is sent!");

#[cfg(target_os = "windows")]
compile_error!("This application is not supported on Windows (yet)");

// Re-exports
pub use chrono;
pub use libc;

// mod finderbuilder;
// pub use finderbuilder::FinderBuilder;

#[macro_use]
pub(crate) mod macros;

// mod cli_helpers;

// pub use cli_helpers::{FileTypeParser, SizeFilter, SizeFilterParser, TimeFilter, TimeFilterParser};

// mod iter;
// #[cfg(any(target_os = "linux", target_os = "android"))]
// pub use iter::GetDents;
// pub use iter::ReadDir;

#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub(crate) use libc::{dirent as dirent64, readdir as readdir64};

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) use libc::{dirent64, readdir64};

// mod printer;
// pub(crate) use printer::write_paths_coloured;

mod test;

// mod buffer;
// pub use buffer::{AlignedBuffer, ValueType};

// mod memchr_derivations;
// pub use memchr_derivations::{
//     contains_zero_byte, find_char_in_word, find_last_char_in_word, find_zero_byte_u64, memrchr,
// };

// mod direntry;
// pub use direntry::DirEntry;

mod error;
pub use error::{DirEntryError, FilesystemIOError, SearchConfigError, TraversalError};

// mod types;

// pub use types::FileDes;
// pub use types::Result;

// pub(crate) use types::{DirEntryFilter, FilterType};

// mod util;

// pub(crate) use utils::BytePath;
// #[cfg(any(
//     target_os = "linux",
//     target_os = "android",
//     target_os = "emscripten",
//     target_os = "illumos",
//     target_os = "solaris",
//     target_os = "redox",
//     target_os = "hermit",
//     target_os = "fuchsia",
//     target_os = "macos",
//     target_os = "freebsd",
//     target_os = "dragonfly",
//     target_os = "openbsd",
//     target_os = "netbsd",
//     target_os = "aix",
//     target_os = "hurd"
// ))]
// pub use utils::dirent_const_time_strlen;

mod config;
pub use config::SearchConfig;

// mod glob;
// pub use glob::glob_to_regex;

// mod filetype;
// pub use filetype::FileType;

//this allocator is more efficient than jemalloc through my testing(still better than system allocator)
//miri doesnt support custom allocators
//not sure which platforms support this, BSD doesnt from testing
#[cfg(all(
    any(target_os = "linux", target_os = "android", target_os = "macos"),
    not(miri),
    not(debug_assertions)
))]
//miri doesnt support custom allocators
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc; //Please note, don't  use v3 it has weird bugs. I might try snmalloc in future.

pub mod filters;
pub mod fs;
pub mod util;
pub mod walk;
