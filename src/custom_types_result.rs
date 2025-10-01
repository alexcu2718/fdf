use crate::{AlignedBuffer, DirEntry, DirEntryError, SearchConfig, const_from_env};

///Generic result type for directory entry operations
pub type Result<T> = core::result::Result<T, DirEntryError>;

const_from_env!(
    /// The maximum length of a local path, set to 4096/1024 (Linux/Non-Linux respectively) by default, but can be customised via environment variable.
    LOCAL_PATH_MAX: usize = "LOCAL_PATH_MAX", libc::PATH_MAX
); //set to PATH_MAX, but allow trivial customisation!

//4115==pub const BUFFER_SIZE_LOCAL: usize = crate::offset_of!(libc::dirent64, d_name) + libc::PATH_MAX as usize; //my experiments tend to prefer this. maybe entirely anecdata.
const_from_env!(
    /// The size of the buffer used for directory entries, set to 4115 by default, but can be customised via environment variable.
    /// size of IO block
    BUFFER_SIZE:usize="BUFFER_SIZE",std::mem::offset_of!(libc::dirent, d_name) + libc::PATH_MAX as usize
);
//basically this is the should allow getdents to grab a lot of entries in one go

pub type PathBuffer = AlignedBuffer<u8, LOCAL_PATH_MAX>;
#[cfg(target_os = "linux")] //we only use a buffer for syscalls on linux because of stable ABI
pub type SyscallBuffer = AlignedBuffer<u8, BUFFER_SIZE>;

///filter function type for directory entries,
pub type FilterType = fn(&SearchConfig, &DirEntry, Option<DirEntryFilter>) -> bool;
///generic filter function type for directory entries
pub type DirEntryFilter = fn(&DirEntry) -> bool;
