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

//4115==pub const BUFFER_SIZE_LOCAL: usize = crate::offset_of!(libc::dirent64, d_name) + libc::PATH_MAX as usize; //my experiments tend to prefer this. maybe entirely anecdata.
#[cfg(all(any(target_os = "linux", target_os = "android"), not(debug_assertions)))]
pub const BUFFER_SIZE: usize = 8 * 4096;

#[cfg(all(any(target_os = "linux", target_os = "android"), debug_assertions))]
pub const BUFFER_SIZE: usize = 4096; // Crashes during testing due to parallel processes taking up too much stack

#[cfg(target_os = "macos")]
pub const BUFFER_SIZE: usize = 0x2000; //readdir calls this value for buffer size, look at syscall tracing below (8192)

/*
/tmp/fdf_test2 getdirentries !3 ❯ sudo dtruss  fd -HI . 2>&1 | grep getdirentries | head                  ✘ INT alexc@alexcs-iMac 00:52:24

.git/refs/heads/getdirentries
.git/logs/refs/heads/getdirentries
getdirentries64(0x3, 0x7FD166808A00, 0x2000)             = 896 0
getdirentries64(0x3, 0x7FD166808A00, 0x2000)             = 408 0
getdirentries64(0x3, 0x7FD166808A00, 0x2000)             = 288 0


/tmp/fdf_test getdirentries !8 ❯ sudo dtruss ls . -R 2>&1 | grep getdirentries | head                          alexc@alexcs-iMac 00:58:19

getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 896 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 104 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 1520 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 112 0
getdirentries64(0x3, 0x7FEE86013C00, 0x2000)             = 344 0

*/
#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
const_assert!(BUFFER_SIZE >= 4096, "Buffer size too small!");
