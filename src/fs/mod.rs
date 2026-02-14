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
