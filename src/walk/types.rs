use crate::{SearchConfig, fs::DirEntry};
use std::os::fd::BorrowedFd;
/// Filter function type for directory entries,
pub type FilterType =
    fn(&SearchConfig, &DirEntry, Option<DirEntryFilter>, Option<BorrowedFd<'_>>) -> bool;
/// Generic filter function type for directory entries
pub type DirEntryFilter = fn(&DirEntry) -> bool;
