use crate::{
    SearchConfig,
    fs::{DirEntry, FileDes},
};

/// Filter function type for directory entries,
pub type FilterType =
    fn(&SearchConfig, &DirEntry, Option<DirEntryFilter>, Option<&FileDes>) -> bool;
/// Generic filter function type for directory entries
pub type DirEntryFilter = fn(&DirEntry) -> bool;
