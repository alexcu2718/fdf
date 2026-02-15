mod buffer;
mod dir_entry;
mod file_type;
mod iter;
mod types;

pub use buffer::{AlignedBuffer, ValueType};
pub use dir_entry::DirEntry;
pub use file_type::FileType;
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris"
))]
pub use iter::GetDents;
#[cfg(any(target_os = "macos", target_os = "freebsd"))]
pub use iter::GetDirEntries;
pub use iter::ReadDir;
pub use types::{FileDes, Result};
