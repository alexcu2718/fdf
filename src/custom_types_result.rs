use crate::AlignedBuffer;
use crate::DirEntryError;
use libc::{PATH_MAX, dirent64};
use slimmer_box::SlimmerBox;
use std::mem::offset_of;
use std::sync::Arc;

use std::ops::Deref;
///Generic result type for directory entry operations
pub type Result<T> = std::result::Result<T, DirEntryError>;

pub const LOCAL_PATH_MAX: usize = 512;
//basically this is the should allow getdents to grab a lot of entries in one go
pub const BUFFER_SIZE: usize = offset_of!(dirent64, d_name) + PATH_MAX as usize; //my experiments tend to prefer this. maybe entirely anecdata.

pub type PathBuffer = AlignedBuffer<u8, LOCAL_PATH_MAX>;
pub type SyscallBuffer = AlignedBuffer<u8, BUFFER_SIZE>;

// Define the trait that all storage types must implement (for our main types)
//I can probably extend this more.
pub trait BytesStorage: Deref<Target = [u8]> {
    fn from_slice(bytes: &[u8]) -> Self;
}
// Define a trait for types that can be converted to a byte slice
// This allows us to use different storage types like Arc, Box, Vec, and SlimmerBox
pub trait AsU8 {
    fn as_bytes(&self) -> &[u8];
}

impl AsU8 for SlimmerBox<[u8], u16> {
    fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

impl AsU8 for Arc<[u8]> {
    fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

impl AsU8 for Vec<u8> {
    fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

impl AsU8 for Box<[u8]> {
    fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

// BytesStorage for SlimmerBox
impl BytesStorage for SlimmerBox<[u8], u16> {
    /// # Safety
    /// The input must have a length less than `u16::MAX`
    fn from_slice(bytes: &[u8]) -> Self {
        debug_assert!(
            bytes.len() < u16::MAX as usize,
            "Input bytes length exceeds u16::MAX"
        );
        unsafe { Self::new_unchecked(bytes) }
    }
}

//  BytesStorage for Arc<[u8]>
impl BytesStorage for Arc<[u8]> {
    fn from_slice(bytes: &[u8]) -> Self {
        Self::from(bytes)
    }
}

//BytesStorage for Vec<[u8]>
impl BytesStorage for Vec<u8> {
    fn from_slice(bytes: &[u8]) -> Self {
        bytes.to_vec()
    }
}

// BytesStorage for Box<[u8]>
impl BytesStorage for Box<[u8]> {
    fn from_slice(bytes: &[u8]) -> Self {
        Self::from(bytes)
    }
}

// OsBytes generic over the storage type, this allows easy switch to arc for multithreading to avoid race conditions:)
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
        self.bytes.as_ref()
    }

    #[inline]
    #[must_use]
    /// Returns a reference to the underlying bytes as  `&Path`
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_path(&self) -> &std::path::Path {
        self.as_os_str().as_ref()
    }

    #[inline]
    #[must_use]
    #[allow(clippy::transmute_ptr_to_ptr)]
    /// Returns a reference to the underlying bytes as an `OsStr`.
    /// This is unsafe because it assumes the bytes are valid UTF-8. but as this is on linux its fine.
    pub fn as_os_str(&self) -> &std::ffi::OsStr {
        //transmute is safe because osstr <=> bytes on linux (NOT windows)
        unsafe { std::mem::transmute(self.as_bytes()) }
    }
}

impl<S: BytesStorage, T: AsRef<[u8]>> From<T> for OsBytes<S> {
    #[inline]
    fn from(data: T) -> Self {
        Self::new(data.as_ref())
    }
}
//generic examples of types :)
#[allow(dead_code)]
pub type SlimOsBytes = OsBytes<SlimmerBox<[u8], u16>>;
#[allow(dead_code)]
pub type ArcOsBytes = OsBytes<std::sync::Arc<[u8]>>;
