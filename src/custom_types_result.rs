use crate::const_from_env;
use crate::{AlignedBuffer, DirEntry, DirEntryError, SearchConfig};
use core::ops::Deref;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use slimmer_box::SlimmerBox;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
///Generic result type for directory entry operations
pub type Result<T> = core::result::Result<T, DirEntryError>;

const_from_env!(
    /// The maximum length of a local path, set to 4096 by default, but can be customised via environment variable.
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
#[allow(dead_code)] //this should be only linux only (because of getdents )
pub type SyscallBuffer = AlignedBuffer<u8, BUFFER_SIZE>;

///  a trait that all storage types must implement (for our main types) (so the user can use their own types if they want)
pub trait BytesStorage: Deref<Target = [u8]> {
    fn from_slice(bytes: &[u8]) -> Self;
}
// Define a trait for types that can be converted to a byte slice
// This allows us to use different storage types like Arc, Box, Vec, and SlimmerBox

// BytesStorage for SlimmerBox
#[cfg(any(target_os = "linux", target_os = "macos"))]
impl BytesStorage for SlimmerBox<[u8], u16> {
    /// # Safety
    /// The input must have a length less than `LOCAL_PATH_MAX`
    #[inline]
    fn from_slice(bytes: &[u8]) -> Self {
        debug_assert!(
            bytes.len() < crate::LOCAL_PATH_MAX,
            "Input bytes length exceeds u16::MAX"
        );
        unsafe { Self::new_unchecked(bytes) }
    }
}
//through this macro one can implement it for their own types yay!
crate::impl_bytes_storage!(Arc<[u8]>, Vec<u8>, Box<[u8]>);

/// `OsBytes` provides a generic wrapper around byte storage types that implement the `BytesStorage` trait.
///
/// It allows for easy conversion between byte slices and various storage types, such as `Box`, `Arc`, `Vec`, or `SlimmerBox`.
///  ( switch to arc for multithreading to avoid race conditions:))
#[derive(Clone, Debug)] //#[repr(C, align(8))]
pub struct OsBytes<S: BytesStorage> {
    pub(crate) bytes: S,
}

impl<S: BytesStorage> OsBytes<S> {
    #[inline]
    #[must_use]
    /// Creates a new `OsBytes` from a byte slice with storage backend type S, eg Box/Arc/Vec/SlimmerBox
    pub fn new(bytes: &[u8]) -> Self {
        Self {
            bytes: S::from_slice(bytes),
        }
    }

    #[inline]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    /// Returns a reference to the underlying bytes.
    pub fn as_bytes(&self) -> &[u8] {
        debug_assert!(
            self.bytes.len() < LOCAL_PATH_MAX,
            "the path is longer than LOCAL_PATH_MAX, THIS SHOULDNT HAPPEN"
        );
        &self.bytes
    }

    #[inline]
    #[must_use]
    /// Returns a reference to the underlying bytes as  `&Path`
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_path(&self) -> &Path {
        self.as_os_str().as_ref()
    }

    #[inline]
    #[must_use]
    #[allow(clippy::transmute_ptr_to_ptr)]
    /// Returns a reference to the underlying bytes as an `OsStr`.
    /// This is unsafe because it assumes the bytes are valid UTF-8. but as this is on linux its fine.
    pub fn as_os_str(&self) -> &OsStr {
        //transmute is safe because osstr <=> bytes on POSIX (NOT windows)
        unsafe { core::mem::transmute(self.as_bytes()) }
    }
}

unsafe impl<S> Send for OsBytes<S> where S: Send + BytesStorage + 'static {}

impl<S: BytesStorage, T: AsRef<[u8]>> From<T> for OsBytes<S> {
    #[inline]
    fn from(data: T) -> Self {
        Self::new(data.as_ref())
    }
}

///filter function type for directory entries,
pub type FilterType<S> = fn(&SearchConfig, &DirEntry<S>, Option<DirEntryFilter<S>>) -> bool;
///generic filter function type for directory entries
pub type DirEntryFilter<S> = fn(&DirEntry<S>) -> bool;
#[allow(dead_code)]
#[cfg(any(target_os = "linux", target_os = "macos"))]
/// This is a type alias for a boxed slice of bytes with a slimmer size representation on Linux/macos, 10 bytes not 16
pub type SlimmerBytes = SlimmerBox<[u8], u16>;
#[cfg(not(any(target_os = "linux", target_os = "macos")))] // If not on Linux/macos, we use a regular Box
pub type SlimmerBytes = Box<[u8]>;
