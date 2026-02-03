mod buffer;
mod dir_entry;
mod file_type;
mod iter;
mod types;

pub use buffer::{AlignedBuffer, ValueType};
pub use dir_entry::DirEntry;
pub use file_type::FileType;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub use iter::GetDents;
#[cfg(target_os = "macos")]
pub use iter::GetDirEntries;
pub use iter::ReadDir;
pub use types::{FileDes, Result};

#[cfg(all(any(target_os = "linux", target_os = "android"), not(debug_assertions)))]
pub const BUFFER_SIZE: usize = 8 * 4096;

#[cfg(all(any(target_os = "linux", target_os = "android"), debug_assertions))]
pub const BUFFER_SIZE: usize = 4096; // Crashes during testing due to parallel processes taking up too much stack

#[cfg(all(target_os = "macos", not(debug_assertions)))]
pub const BUFFER_SIZE: usize = 0x2000; //readdir calls this value for buffer size, look at syscall tracing below (8192)

#[cfg(all(target_os = "macos", debug_assertions))]
pub const BUFFER_SIZE: usize = 0x1000; // Give a smaller size to avoid stack overflow when going on tests

/*
/tmp/fdf_test getdirentries ❯ sudo dtruss  fd -HI . 2>&1 | grep getdirentries | head                  ✘ INT alexc@alexcs-iMac 00:52:24


getdirentries64(0x3, 0x7FD166808A00, 0x2000)             = 896 0
getdirentries64(0x3, 0x7FD166808A00, 0x2000)             = 408 0
getdirentries64(0x3, 0x7FD166808A00, 0x2000)             = 288 0


/tmp/fdf_test getdirentries  ❯ sudo dtruss ls . -R 2>&1 | grep getdirentries | head                          alexc@alexcs-iMac 00:58:19

getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 896 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 104 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 1520 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 112 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 344 0

*/
#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
const_assert!(BUFFER_SIZE >= 4096, "Buffer size too small!");
