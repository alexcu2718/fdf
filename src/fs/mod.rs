mod dir_entry;
mod file_type;
mod iter;
mod owned_dir;
mod types;
mod unique_dirent;
pub use owned_dir::{DirStream, OwnedDir};

#[cfg(has_faster_readdir)]
pub use {iter::GetDents, owned_dir::OwnedFdDir, types::BUFFER_SIZE};

#[cfg(target_os = "android")]
pub(crate) const AT_EACCESS: libc::c_int = 0; // android being weird, again....
#[cfg(not(target_os = "android"))]
pub(crate) use libc::AT_EACCESS;

pub use dir_entry::DirEntry;
pub use file_type::FileType;
pub use iter::{DirIter, ReadDir};
pub use types::Result;
