use crate::BytePath as _;
use crate::cli_helpers::SizeFilter;
use crate::glob_to_regex;
use crate::{DirEntry, FileType, SearchConfigError};
use core::num::NonZeroU32;
use regex::bytes::{Regex, RegexBuilder};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// File type filter for directory traversal
#[expect(clippy::exhaustive_enums, reason = "This list is exhaustive")]
pub enum FileTypeFilter {
    /// Regular file
    File,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
    /// Named pipe (FIFO)
    Pipe,
    /// Character device
    CharDevice,
    /// Block device
    BlockDevice,
    /// Socket
    Socket,
    /// Unknown file type
    Unknown,
    /// Executable file
    Executable,
    /// Empty file
    Empty,
}

impl FileTypeFilter {
    /// Converts the file type filter to its corresponding byte representation
    ///
    /// This provides backward compatibility with legacy systems and protocols
    /// that use single-byte codes to represent file types.
    ///
    /// # Returns
    /// A `u8` value representing the file type:
    /// - `b'f'` for regular files
    /// - `b'd'` for directories  
    /// - `b'l'` for symbolic links
    /// - `b'p'` for named pipes (FIFOs)
    /// - `b'c'` for character devices
    /// - `b'b'` for block devices
    /// - `b's'` for sockets
    /// - `b'u'` for unknown file types
    /// - `b'x'` for executable files
    /// - `b'e'` for empty files
    ///
    /// # Examples
    /// ```
    /// # use fdf::FileTypeFilter;
    /// let filter = FileTypeFilter::File;
    /// assert_eq!(filter.as_byte(), b'f');
    ///
    /// let filter = FileTypeFilter::Directory;
    /// assert_eq!(filter.as_byte(), b'd');
    /// ```
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        match self {
            Self::File => b'f',
            Self::Directory => b'd',
            Self::Symlink => b'l',
            Self::Pipe => b'p',
            Self::CharDevice => b'c',
            Self::BlockDevice => b'b',
            Self::Socket => b's',
            Self::Unknown => b'u',
            Self::Executable => b'x',
            Self::Empty => b'e',
        }
    }

    /// Parses a character into a `FileTypeFilter`
    ///
    /// This method converts a single character into the corresponding file type filter,
    /// which is useful for parsing command-line arguments or configuration files.
    ///
    /// # Parameters
    /// - `c`: The character to parse into a file type filter
    ///
    /// # Returns
    /// - `Ok(FileTypeFilter)` if the character represents a valid file type
    /// - `Err(String)` with an error message if the character is invalid
    ///
    /// # Supported Characters
    /// - `'d'` - Directory
    /// - `'u'` - Unknown file type  
    /// - `'l'` - Symbolic link
    /// - `'f'` - Regular file
    /// - `'p'` - Named pipe (FIFO)
    /// - `'c'` - Character device
    /// - `'b'` - Block device
    /// - `'s'` - Socket
    /// - `'e'` - Empty file
    /// - `'x'` - Executable file
    ///
    /// # Examples
    /// ```
    /// # use fdf::FileTypeFilter;
    /// assert!(FileTypeFilter::from_char('d').is_ok());
    /// assert!(FileTypeFilter::from_char('f').is_ok());
    /// assert!(FileTypeFilter::from_char('z').is_err()); // Invalid character
    ///
    /// let filter = FileTypeFilter::from_char('l').unwrap();
    /// assert!(matches!(filter, FileTypeFilter::Symlink));
    /// ```
    ///
    /// # Errors
    /// Returns an error if the character does not correspond to any known file type.
    /// The error message includes the invalid character and suggests using `--help`
    /// to see valid types.
    pub fn from_char(c: char) -> core::result::Result<Self, String> {
        match c {
            'd' => Ok(Self::Directory),
            'u' => Ok(Self::Unknown),
            'l' => Ok(Self::Symlink),
            'f' => Ok(Self::File),
            'p' => Ok(Self::Pipe),
            'c' => Ok(Self::CharDevice),
            'b' => Ok(Self::BlockDevice),
            's' => Ok(Self::Socket),
            'e' => Ok(Self::Empty),
            'x' => Ok(Self::Executable),
            _ => Err(format!(
                "Invalid file type: '{c}'. See --help for valid types."
            )),
        }
    }
}
#[derive(Clone, Debug)]
#[expect(clippy::struct_excessive_bools, reason = "It's a CLI tool.")]
/// This struct holds the configuration for searching directories.
///
///
/// It includes options for regex matching, hiding hidden files, keeping directories,
/// matching file extensions, whether to search file names only, depth of search,
/// and whether to follow symlinks.
pub struct SearchConfig {
    pub(crate) regex_match: Option<Regex>,
    ///a regex to match against the file names
    ///if this is None, then the pattern is empty or just a dot, so we
    ///match everything, otherwise we match against the regex
    pub(crate) hide_hidden: bool,
    ///if true, then we hide hidden files (those starting with a dot)
    pub(crate) keep_dirs: bool,
    ///if true, then we keep directories in the results, otherwise we only return non-directory files
    pub(crate) extension_match: Option<Box<[u8]>>,
    ///if this is Some, then we match against the extension of the file otherwise accept (if none)
    pub(crate) file_name_only: bool,
    ///if true, then we only match against the file name, otherwise we match against the full path when regexing
    pub(crate) depth: Option<NonZeroU32>,
    ///the maximum depth to search, if None then no limit
    pub(crate) follow_symlinks: bool, //if true, then we follow symlinks, otherwise we do not follow them
    /// a size filter
    pub(crate) size_filter: Option<SizeFilter>,
    /// a type filter
    pub(crate) type_filter: Option<FileTypeFilter>,
    ///if true, then we show errors during traversal
    pub(crate) show_errors: bool,
}

impl SearchConfig {
    // Constructor for SearchConfig
    // Builds a regex matcher if a valid pattern is provided, otherwise stores None
    // Returns an error if the regex compilation fails
    #[expect(clippy::fn_params_excessive_bools, reason = "Internal convenience")]
    #[expect(clippy::too_many_arguments, reason = "Internal convenience")]
    pub(crate) fn new<A: AsRef<str>>(
        pattern: Option<&A>,
        hide_hidden: bool,
        case_insensitive: bool,
        keep_dirs: bool,
        filenameonly: bool,
        extension_match: Option<Box<[u8]>>,
        depth: Option<NonZeroU32>,
        follow_symlinks: bool,
        size_filter: Option<SizeFilter>,
        type_filter: Option<FileTypeFilter>,
        show_errors: bool,
        use_glob: bool,
    ) -> core::result::Result<Self, SearchConfigError> {
        let (file_name_only, pattern_to_use) = if let Some(patt_ref) = pattern.as_ref() {
            let patt = patt_ref.as_ref();
            let file_name_only = if patt.contains('/') {
                false //Over ride because if it's got a slash, it's gotta be a full path
            } else {
                filenameonly
            };

            let pattern_to_use = if use_glob {
                glob_to_regex(patt).map_err(SearchConfigError::GlobToRegexError)?
            } else {
                patt.into()
            };
            (file_name_only, pattern_to_use)
        } else {
            // No pattern provided, use match-all pattern
            (filenameonly, ".*".into())
        };

        // If pattern is "." or empty, we do not filter by regex, this avoids building a regex (even if its trivial cost)
        let regex_match =
            if pattern_to_use == "." || pattern_to_use == ".*" || pattern_to_use.is_empty() {
                None
            } else {
                let reg = RegexBuilder::new(&pattern_to_use)
                    .case_insensitive(case_insensitive)
                    .dot_matches_new_line(false)
                    .build();

                if let Err(regerror) = reg {
                    return Err(SearchConfigError::RegexError(regerror));
                }
                reg.ok()
            };

        Ok(Self {
            regex_match,
            hide_hidden,
            keep_dirs,
            extension_match,
            file_name_only,
            depth,
            follow_symlinks,
            size_filter,
            type_filter,
            show_errors,
        })
    }

    #[inline]
    #[must_use]
    /// Evaluates a custom predicate function against a path
    pub fn matches_with<F: Fn(&[u8]) -> bool>(&self, path: &[u8], predicate: F) -> bool {
        predicate(path)
    }

    #[inline]
    /// Checks for extension match via memchr
    pub fn matches_extension<S>(&self, entry: &S) -> bool
    where
        S: core::ops::Deref<Target = [u8]>,
    {
        debug_assert!(
            !entry.contains(&b'/'),
            "the filename contains a slash, some arithmetic has gone wrong somewhere!"
        ); // ensure that the entry is a file name and not a path
        self.extension_match
            .as_ref()
            .is_none_or(|ext| entry.matches_extension(ext))
    }

    #[inline]
    #[must_use]
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only checking regular files"
    )]
    #[allow(clippy::cast_sign_loss)] //signloss dont matter here
    /// Applies the configured size filter to a directory entry, if any.
    /// For regular files the size is checked directly.
    /// For symlinks, the target is resolved first and then checked if it is a regular file.
    /// Other file types are ignored.
    pub fn matches_size(&self, entry: &DirEntry) -> bool {
        let Some(filter_size) = self.size_filter else {
            return true; // No filter means always match
        };

        match entry.file_type {
            FileType::RegularFile => entry
                .file_size()
                .ok()
                .is_some_and(|sz| filter_size.is_within_size(sz as _)),

            FileType::Symlink => {
                if let Ok(path) = entry.to_full_path() {
                    if path.is_regular_file() {
                        if let Ok(sz) = entry.file_size() {
                            return filter_size.is_within_size(sz as _);
                        }
                    }
                }
                false
            }

            _ => false,
        }
    }
    #[inline]
    #[must_use]
    /// Applies a type filter using `FileTypeFilter` enum
    /// Supports common file types: file, dir, symlink, device, pipe, etc
    pub fn matches_type(&self, entry: &DirEntry) -> bool {
        let Some(type_filter) = self.type_filter else {
            return true;
        };

        match type_filter {
            FileTypeFilter::File => entry.is_regular_file(),
            FileTypeFilter::Directory => entry.is_dir(),
            FileTypeFilter::Symlink => entry.is_symlink(),
            FileTypeFilter::Pipe => entry.is_pipe(),
            FileTypeFilter::CharDevice => entry.is_char_device(),
            FileTypeFilter::BlockDevice => entry.is_block_device(),
            FileTypeFilter::Socket => entry.is_socket(),
            FileTypeFilter::Unknown => entry.is_unknown(),
            FileTypeFilter::Executable => entry.is_executable(),
            FileTypeFilter::Empty => entry.is_empty(),
        }
    }

    #[inline]
    #[must_use]
    #[expect(clippy::cast_lossless, reason = "overcomplicates it")]
    #[expect(clippy::indexing_slicing, reason = "used for debug assert")]
    /// Checks if the path or file name matches the regex filter
    /// If `full_path` is false, only checks the filename
    ///
    /// Use a branchless trick to do indexing
    pub fn matches_path(&self, dir: &DirEntry, full_path: bool) -> bool {
        debug_assert!(
            !dir.file_name().contains(&b'/'),
            "file_name contains a directory separator"
        );

        debug_assert!(
            &dir.as_bytes()[dir.file_name_index()..] == dir.file_name(),
            "showing the below works"
        );

        self.regex_match.as_ref().is_none_or(|reg|
                // use arithmetic to avoid branching costs 
             { let index_amount=!full_path as usize * dir.file_name_index();
                     // SAFETY: are always indexing within bounds.
            unsafe{reg.is_match(dir.get_unchecked(index_amount..))}
            })
    }
}
