use crate::{SearchConfig, fs::DirEntry};

/// Filter function type for directory entries,
pub type FilterType = fn(&SearchConfig, &DirEntry, Option<DirEntryFilter>) -> bool;
/// Generic filter function type for directory entries
pub type DirEntryFilter = fn(&DirEntry) -> bool;
