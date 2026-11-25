mod buffer;
mod dir_entry;
mod file_type;
mod iter;
mod owned_dir;
mod types;
pub use buffer::{AlignedBuffer, ValueType};
#[cfg(has_faster_readdir)]
pub use owned_dir::OwnedFdDir;
pub use owned_dir::{DirStream, OwnedDir};

#[cfg(target_os = "android")]
pub(crate) const AT_EACCESS: libc::c_int = 0; // android being weird, again....
#[cfg(not(target_os = "android"))]
pub(crate) use libc::AT_EACCESS;

pub use dir_entry::DirEntry;
pub use file_type::FileType;
pub use iter::{DirIter, ReadDir};
pub use types::Result;

#[cfg(has_faster_readdir)]
pub use iter::GetDents;
#[cfg(has_faster_readdir)]
pub use types::SyscallBuffer;
