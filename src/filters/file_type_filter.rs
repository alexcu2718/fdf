use clap::{
    Arg, Command, Error,
    builder::{PossibleValue, TypedValueParser},
    error::{ContextKind, ContextValue, ErrorKind},
};
use std::ffi::OsStr;

/// File type filter for directory traversal
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /**
     Converts the file type filter to its corresponding byte representation

     This provides backward compatibility with legacy systems and protocols
     that use single-byte codes to represent file types.

     # Returns
     A `u8` value representing the file type:
     - `b'f'` for regular files
     - `b'd'` for directories
     - `b'l'` for symbolic links
     - `b'p'` for named pipes (FIFOs)
     - `b'c'` for character devices
     - `b'b'` for block devices
     - `b's'` for sockets
     - `b'u'` for unknown file types
     - `b'x'` for executable files
     - `b'e'` for empty files

     # Examples
     ```
     # use fdf::FileTypeFilter;
     let filter = FileTypeFilter::File;
     assert_eq!(filter.as_byte(), b'f');

     let filter = FileTypeFilter::Directory;
     assert_eq!(filter.as_byte(), b'd');
     ```
    */
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

    /**
     Parses a character into a `FileTypeFilter`

     This method converts a single character into the corresponding file type filter,
     which is useful for parsing command-line arguments or configuration files.

     # Parameters
     - `c`: The character to parse into a file type filter

     # Returns
     - `Ok(FileTypeFilter)` if the character represents a valid file type
     - `Err(String)` with an error message if the character is invalid

     # Supported Characters
     - `'d'` - Directory
     - `'u'` - Unknown file type
     - `'l'` - Symbolic link
     - `'f'` - Regular file
     - `'p'` - Named pipe (FIFO)
     - `'c'` - Character device
     - `'b'` - Block device
     - `'s'` - Socket
     - `'e'` - Empty file
     - `'x'` - Executable file

     # Examples
     ```
     # use fdf::FileTypeFilter;
     assert!(FileTypeFilter::from_char('d').is_ok());
     assert!(FileTypeFilter::from_char('f').is_ok());
     assert!(FileTypeFilter::from_char('z').is_err()); // Invalid character

     let filter = FileTypeFilter::from_char('l').unwrap();
     assert!(matches!(filter, FileTypeFilter::Symlink));
     ```

     # Errors
     Returns an error if the character does not correspond to any known file type.
     The error message includes the invalid character and suggests using `--help`
     to see valid types.
    */
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

/// A struct to provide completions for filetype completions in CLI
#[derive(Clone, Debug)]
#[allow(clippy::exhaustive_structs)]
pub struct FileTypeFilterParser;

impl TypedValueParser for FileTypeFilterParser {
    type Value = FileTypeFilter;

    fn parse_ref(
        &self,
        cmd: &Command,
        _arg: Option<&Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, Error> {
        let value_str = value
            .to_str()
            .ok_or_else(|| Error::new(ErrorKind::InvalidUtf8).with_cmd(cmd))?;

        match value_str.to_lowercase().as_str() {
            "d" | "dir" | "hardlink" | "directory" => Ok(FileTypeFilter::Directory),
            "u" | "unknown" => Ok(FileTypeFilter::Unknown),
            "l" | "symlink" | "link" => Ok(FileTypeFilter::Symlink),
            "f" | "file" | "regular" => Ok(FileTypeFilter::File),
            "p" | "pipe" | "fifo" => Ok(FileTypeFilter::Pipe),
            "c" | "char" | "chardev" | "chardevice" => Ok(FileTypeFilter::CharDevice),
            "b" | "block" | "blockdev" | "blockdevice" => Ok(FileTypeFilter::BlockDevice),
            "s" | "socket" | "sock" => Ok(FileTypeFilter::Socket),
            "e" | "empty" => Ok(FileTypeFilter::Empty),
            "x" | "exec" | "executable" => Ok(FileTypeFilter::Executable),
            _ => {
                let mut error = Error::new(ErrorKind::InvalidValue).with_cmd(cmd);

                error.insert(
                    ContextKind::InvalidValue,
                    ContextValue::String(format!("invalid file type: '{value_str}'")),
                );

                error.insert(
                    ContextKind::SuggestedValue,
                    ContextValue::Strings(vec![
                        "d".into(),
                        "f".into(),
                        "l".into(),
                        "s".into(),
                        "p".into(),
                    ]),
                );

                // All valid values
                error.insert(
                    ContextKind::ValidValue,
                    ContextValue::Strings(vec![
                        "d, dir, directory, hardlink".into(),
                        "u, unknown".into(),
                        "l, symlink, link".into(),
                        "f, file, regular".into(),
                        "p, pipe, fifo".into(),
                        "c, char, chardev".into(),
                        "b, block, blockdev".into(),
                        "s, socket".into(),
                        "e, empty".into(),
                        "x, exec, exe ,executable".into(),
                    ]),
                );

                Err(error)
            }
        }
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            [
                PossibleValue::new("d")
                    .aliases(["dir", "directory", "hardlink"])
                    .help("Directory"),
                PossibleValue::new("u")
                    .aliases(["unknown"])
                    .help("Unknown type"),
                PossibleValue::new("l")
                    .aliases(["symlink", "link"])
                    .help("Symbolic link"),
                PossibleValue::new("f")
                    .aliases(["file", "regular"])
                    .help("Regular file"),
                PossibleValue::new("p")
                    .aliases(["pipe", "fifo"])
                    .help("Pipe/FIFO"),
                PossibleValue::new("c")
                    .aliases(["char", "chardev"])
                    .help("Character device"),
                PossibleValue::new("b")
                    .aliases(["block", "blockdev", "block-device"])
                    .help("Block device"),
                PossibleValue::new("s")
                    .aliases(["socket", "sock"])
                    .help("Socket"),
                PossibleValue::new("e")
                    .aliases(["empty"])
                    .help("Empty file"),
                PossibleValue::new("x")
                    .aliases(["exec", "executable", "exe"])
                    .help("Executable file"),
            ]
            .into_iter(),
        ))
    }
}
