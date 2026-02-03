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
#[cfg(any(target_os = "linux", target_os = "android"))]
const_from_env!(
    /// The size of the buffer used for directory entries, set to 4120 by default, but can be customised via environment variable.
    /// Meant to be above the size of a page basically
    BUFFER_SIZE:usize="BUFFER_SIZE",(std::mem::offset_of!(crate::dirent64, d_name) + libc::PATH_MAX as usize).next_multiple_of(8)
); //TODO investigate this more!
//basically this is the should allow getdents to grab a lot of entries in one go

/*

Interestingly getdents via readdir wrapper uses a much bigger buffer, which I have tested and is significantly less performant.

λ  strace -fn fd NOMATCHLOL --threads 1 2>&1 | grep getdents | head
[pid 123763] [ 217] getdents64(3, 0x7fb934000d30 /* 85 entries */, 32768) = 2920
[pid 123763] [ 217] getdents64(3, 0x7fb934000d30 /* 0 entries */, 32768) = 0
[pid 123763] [ 217] getdents64(3, 0x7fb934000d30 /* 54 entries */, 32768) = 2032
[pid 123763] [ 217] getdents64(3, 0x7fb934000d30 /* 0 entries */, 32768) = 0
[pid 123763] [ 217] getdents64(3, 0x7fb934000d30 /* 8 entries */, 32768) = 264
[pid 123763] [ 217] getdents64(3, 0x7fb934000d30 /* 0 entries */, 32768) = 0
[pid 123763] [ 217] getdents64(3, 0x7fb934000d30 /* 4 entries */, 32768) = 96
[pid 123763] [ 217] getdents64(3, 0x7fb934000d30 /* 0 entries */, 32768) = 0
[pid 123763] [ 217] getdents64(3, 0x7fb934000d30 /* 3 entries */, 32768) = 80
[pid 123763] [ 217] getdents64(3, 0x7fb934000d30 /* 0 entries */, 32768) = 0
*/

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
