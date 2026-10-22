mod buffer;
mod dir_entry;
mod file_type;
mod iter;
mod types;

pub use buffer::{AlignedBuffer, ValueType};

#[cfg(target_os = "android")]
pub(crate) const AT_EACCESS: libc::c_int = 0; // android being weird, again....
#[cfg(not(target_os = "android"))]
pub(crate) use libc::AT_EACCESS;

pub use dir_entry::DirEntry;
pub use file_type::FileType;
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "macos",
    target_os = "freebsd"
))]
pub use iter::GetDents;
pub use iter::ReadDir;
pub use types::Result;

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "illumos",
    target_os = "solaris",
    target_os = "macos",
    target_os = "freebsd"
))]
pub use types::SyscallBuffer;
