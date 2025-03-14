#[derive(Debug)]
pub enum DirEntryError {
    InvalidPath,
    InvalidStat,
    TimeError,
    MetadataError,
    Utf8Error(std::str::Utf8Error),
    BrokenPipe(std::io::Error),
}
