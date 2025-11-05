#![allow(clippy::missing_errors_doc)]
pub const KILO: u64 = 1000;
pub const MEGA: u64 = KILO * 1000;
pub const GIGA: u64 = MEGA * 1000;
pub const TERA: u64 = GIGA * 1000;

pub const KIBI: u64 = 1024;
pub const MEBI: u64 = KIBI * 1024;
pub const GIBI: u64 = MEBI * 1024;
pub const TEBI: u64 = GIBI * 1024;
use crate::FileTypeFilter;
use clap::Arg;
use clap::Command;
use clap::Error;
use clap::builder::PossibleValue;
use clap::builder::TypedValueParser;
use clap::error::ContextKind;
use clap::error::ContextValue;
use clap::error::ErrorKind;
use core::fmt;
use core::time::Duration;
use std::ffi::OsStr;
use std::time::SystemTime;
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_enums)]
pub enum ParseSizeError {
    Empty,
    InvalidNumber,
    InvalidUnit,
    InvalidFormat,
}

impl fmt::Display for ParseSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Empty => write!(f, "empty size string"),
            Self::InvalidNumber => write!(f, "invalid number"),
            Self::InvalidUnit => write!(f, "invalid unit"),
            Self::InvalidFormat => write!(f, "invalid format"),
        }
    }
}

impl core::error::Error for ParseSizeError {}
/**
 A filter for file sizes based on various comparison operations.

 # Examples

 ```
 use fdf::SizeFilter;

 // Files larger than 1MB
 let filter = SizeFilter::from_string("+1MB").unwrap();
 assert!(filter.is_within_size(2_000_000)); // 2MB passes
 assert!(!filter.is_within_size(500_000));  // 500KB fails

 // Files exactly 500 bytes
 let filter = SizeFilter::from_string("500").unwrap();
 assert!(filter.is_within_size(500));
 assert!(!filter.is_within_size(501));
 ```
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::exhaustive_enums)]
pub enum SizeFilter {
    /// Maximum size (inclusive): files must be <= this size
    Max(u64),
    /// Minimum size (inclusive): files must be >= this size  
    Min(u64),
    /// Exact size: files must be exactly this size
    Equals(u64),
}

impl SizeFilter {
    /**
     Parses a size string and returns a `SizeFilter`

     # Arguments

     * `s` - A string slice containing the size specification

     # Returns

     * `Ok(SizeFilter)` - If parsing was successful
     * `Err(ParseSizeError)` - If the string couldn't be parsed

     # Errors

     Returns `ParseSizeError` in the following cases:
     - `ParseSizeError::Empty` if the input string is empty
     - `ParseSizeError::InvalidNumber` if the numeric portion is invalid
     - `ParseSizeError::InvalidUnit` if the unit suffix is unrecognized
     - `ParseSizeError::InvalidFormat` if the overall format doesn't match expectations

     # Format

     The expected format is: `[+|=]?<number>[unit]?`
     - `+` prefix: minimum size filter (files >= size)
     - `=` prefix: exact size filter (files == size)
     - No prefix: maximum size filter (files <= size)
     - Supported units: K, M, G, T (metric) and Ki, Mi, Gi, Ti (binary)
     - Default unit is bytes if no unit specified
    */
    pub fn from_string(s: &str) -> Result<Self, ParseSizeError> {
        Self::parse_args(s).ok_or(ParseSizeError::InvalidFormat)
    }
    fn parse_args(start: &str) -> Option<Self> {
        let s = start.trim();
        if s.is_empty() {
            return None;
        }

        let (limit, remaining) = s
            .strip_prefix('+')
            .map(|stripped| ("+", stripped))
            .or_else(|| s.strip_prefix('-').map(|stripped| ("-", stripped)))
            .unwrap_or(("", s));

        let (quantity, unit_str) = Self::parse_size_parts(remaining)?;

        let multiplier = Self::unit_multiplier(&unit_str)?;

        let size = quantity * multiplier;
        match limit {
            "+" => Some(Self::Min(size)),
            "-" => Some(Self::Max(size)),
            "" => Some(Self::Equals(size)),
            _ => None,
        }
    }
    fn parse_size_parts(start: &str) -> Option<(u64, String)> {
        let s = start.trim().to_lowercase();
        let ref_s = s.as_str();

        let digit_end = ref_s
            .chars()
            .position(|c| !c.is_ascii_digit())
            .unwrap_or(s.len());

        if digit_end == ref_s.len() {
            let quantity = s.parse().ok()?;
            return Some((quantity, "b".into()));
        }

        let (num_str, unit_str) = ref_s.split_at(digit_end);
        let quantity = num_str.parse().ok()?;

        Some((quantity, unit_str.into()))
    }
    fn unit_multiplier(unit: &str) -> Option<u64> {
        let unit_lower = unit.trim().to_lowercase();
        match unit_lower.as_ref() {
            "b" => Some(1),
            "k" | "kb" => Some(KILO),
            "ki" | "kib" => Some(KIBI),
            "m" | "mb" => Some(MEGA),
            "mi" | "mib" => Some(MEBI),
            "g" | "gb" => Some(GIGA),
            "gi" | "gib" => Some(GIBI),
            "t" | "tb" => Some(TERA),
            "ti" | "tib" => Some(TEBI),
            _ => None,
        }
    }
    #[must_use]
    pub const fn is_within_size(&self, size: u64) -> bool {
        match *self {
            Self::Max(limit) => size <= limit,
            Self::Min(limit) => size >= limit,
            Self::Equals(limit) => size == limit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_enums)]
pub enum ParseTimeError {
    Empty,
    InvalidNumber,
    InvalidUnit,
    InvalidFormat,
    InvalidTimestamp,
}

impl fmt::Display for ParseTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Empty => write!(f, "empty time string"),
            Self::InvalidNumber => write!(f, "invalid number"),
            Self::InvalidUnit => write!(f, "invalid time unit"),
            Self::InvalidFormat => write!(f, "invalid format"),
            Self::InvalidTimestamp => write!(f, "invalid timestamp"),
        }
    }
}

impl core::error::Error for ParseTimeError {}

/**
 A filter for file modification times.

 # Examples

 ```
 use fdf::TimeFilter;

 // Files modified within the last hour
 let filter = TimeFilter::from_string("-1h").unwrap();

 // Files modified more than 2 days ago
 let filter = TimeFilter::from_string("+2d").unwrap();
 ```
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::exhaustive_enums)]
pub enum TimeFilter {
    /// Files modified before this time (older than)
    Before(SystemTime),
    /// Files modified after this time (newer than)
    After(SystemTime),
    /// Files modified between these times
    Between(SystemTime, SystemTime),
}

impl TimeFilter {
    /**
     Parses a time string and returns a `TimeFilter`

     # Arguments

     * `s` - A string slice containing the time specification

     # Format

     The expected format is: `[+|-]<number><unit>` or for between: `<number><unit>..<number><unit>`
     - `-` prefix: files modified within the last X time (After/newer than)
     - `+` prefix: files modified more than X time ago (Before/older than)
     - `..` separator: between two times (e.g., "2d..1d" = files modified between 2 days and 1 day ago)
     - Supported units: s (seconds), m (minutes), h (hours), d (days), w (weeks), y (years)

    */
    pub fn from_string(s: &str) -> Result<Self, ParseTimeError> {
        Self::parse_args(s).ok_or(ParseTimeError::InvalidFormat)
    }

    fn parse_args(start: &str) -> Option<Self> {
        let s = start.trim();
        if s.is_empty() {
            return None;
        }

        // Check for between format (contains "..")
        if let Some((before_str, after_str)) = s.split_once("..") {
            let before_time = Self::parse_relative_time(before_str.trim())?;
            let after_time = Self::parse_relative_time(after_str.trim())?;

            // Ensure before_time is actually before after_time
            let (older, newer) = if before_time > after_time {
                (before_time, after_time)
            } else {
                (after_time, before_time)
            };

            return Some(Self::Between(newer, older));
        }

        // Parse single time with prefix
        let (prefix, remaining) = s
            .strip_prefix('+')
            .map(|stripped| ("+", stripped))
            .or_else(|| s.strip_prefix('-').map(|stripped| ("-", stripped)))
            .unwrap_or(("", s));

        let time = Self::parse_relative_time(remaining)?;

        match prefix {
            "+" => Some(Self::Before(time)),     // Older than X time ago
            "-" | "" => Some(Self::After(time)), // Newer than X time ago
            _ => None,
        }
    }

    fn parse_relative_time(start_str: &str) -> Option<SystemTime> {
        let s = start_str.trim().to_lowercase();

        // Find where digits end
        let digit_end = s.chars().position(|c| !c.is_ascii_digit())?;

        let (num_str, unit_str) = s.split_at(digit_end);
        let quantity: u64 = num_str.parse().ok()?;

        let duration = match unit_str.trim() {
            "s" | "sec" | "second" | "seconds" => Duration::from_secs(quantity),
            "m" | "min" | "minute" | "minutes" => Duration::from_secs(quantity * 60),
            "h" | "hour" | "hours" => Duration::from_secs(quantity * 3600),
            "d" | "day" | "days" => Duration::from_secs(quantity * 86400),
            "w" | "week" | "weeks" => Duration::from_secs(quantity * 604_800),
            "y" | "year" | "years" => Duration::from_secs(quantity * 31_536_000),
            _ => return None,
        };

        SystemTime::now().checked_sub(duration)
    }

    #[must_use]
    pub fn matches_time(&self, file_time: SystemTime) -> bool {
        match *self {
            Self::Before(cutoff) => {
                // File should be older than cutoff (modified before cutoff)
                file_time.duration_since(cutoff).is_err()
            }
            Self::After(cutoff) => {
                // File should be newer than cutoff (modified after cutoff)
                cutoff.duration_since(file_time).is_err()
            }
            Self::Between(newer, older) => {
                // File should be between newer and older
                let after_newer = newer.duration_since(file_time).is_err();
                let before_older = file_time.duration_since(older).is_err();
                after_newer && before_older
            }
        }
    }
}

/// A Custom parser that provides helpful error messages and suggestions
#[derive(Clone, Debug)]
#[allow(clippy::exhaustive_structs)]
pub struct TimeFilterParser;

impl TypedValueParser for TimeFilterParser {
    type Value = TimeFilter;

    fn parse_ref(
        &self,
        cmd: &Command,
        _arg: Option<&Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, Error> {
        let value_str = value
            .to_str()
            .ok_or_else(|| Error::new(ErrorKind::InvalidUtf8).with_cmd(cmd))?;

        match TimeFilter::from_string(value_str) {
            Ok(filter) => Ok(filter),
            Err(err) => {
                let mut error = Error::new(ErrorKind::InvalidValue).with_cmd(cmd);

                // main error
                error.insert(
                    ContextKind::InvalidValue,
                    ContextValue::String(format!("{err}")),
                );

                // examples as suggestions
                error.insert(
                    ContextKind::SuggestedValue,
                    ContextValue::Strings(vec![
                        "-1h".into(),    // modified within last hour
                        "-30m".into(),   // modified within last 30 minutes
                        "+2d".into(),    // modified more than 2 days ago
                        "+1w".into(),    // modified more than 1 week ago
                        "1d..2h".into(), // modified between 1 day and 2 hours ago
                    ]),
                );

                // Add prefix explanation
                error.insert(
                    ContextKind::Usage,
                    ContextValue::Strings(vec![
                        "Prefixes:".into(),
                        "  -TIME  - files modified within the last TIME (newer)".into(),
                        "  +TIME  - files modified more than TIME ago (older)".into(),
                        "   TIME  - same as -TIME (default)".into(),
                        "  TIME..TIME - files modified between two times".into(),
                    ]),
                );

                // Add additional context
                error.insert(
                    ContextKind::ValidValue,
                    ContextValue::Strings(vec![
                        "s, sec, second, seconds".into(),
                        "m, min, minute, minutes".into(),
                        "h, hour, hours".into(),
                        "d, day, days".into(),
                        "w, week, weeks".into(),
                        "y, year, years".into(),
                    ]),
                );

                Err(error)
            }
        }
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            [
                PossibleValue::new("-1h").help("modified within the last hour"),
                PossibleValue::new("-30m").help("modified within the last 30 minutes"),
                PossibleValue::new("-1d").help("modified within the last day"),
                PossibleValue::new("+2d").help("modified more than 2 days ago"),
                PossibleValue::new("+1w").help("modified more than 1 week ago"),
                PossibleValue::new("1d..2h").help("modified between 1 day and 2 hours ago"),
            ]
            .into_iter(),
        ))
    }
}

/// A Custom parser that provides helpful error messages and suggestions
#[derive(Clone, Debug)]
#[allow(clippy::exhaustive_structs)]
pub struct SizeFilterParser;

impl TypedValueParser for SizeFilterParser {
    type Value = SizeFilter;

    fn parse_ref(
        &self,
        cmd: &Command,
        _arg: Option<&Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, Error> {
        let value_str = value
            .to_str()
            .ok_or_else(|| Error::new(ErrorKind::InvalidUtf8).with_cmd(cmd))?;

        match SizeFilter::from_string(value_str) {
            Ok(filter) => Ok(filter),
            Err(err) => {
                let mut error = Error::new(ErrorKind::InvalidValue).with_cmd(cmd);

                // main error
                error.insert(
                    ContextKind::InvalidValue,
                    ContextValue::String(format!("{err}")),
                );

                // examples as suggestions - clearly showing + and - prefixes
                error.insert(
                    ContextKind::SuggestedValue,
                    ContextValue::Strings(vec![
                        "100".into(),   // exactly 100 bytes
                        "1k".into(),    // exactly 1 kilobyte
                        "+1m".into(),   // larger than 1MB
                        "-500k".into(), // smaller than 500KB
                        "+10mb".into(), // larger than 10MB
                        "-2gib".into(), // smaller than 2GiB
                    ]),
                );

                // Add prefix explanation
                error.insert(
                    ContextKind::Usage,
                    ContextValue::Strings(vec![
                        "Prefixes:".into(),
                        "  +SIZE  - files larger than SIZE".into(),
                        "  -SIZE  - files smaller than SIZE".into(),
                        "   SIZE  - files exactly SIZE (default)".into(),
                    ]),
                );

                // Add valid units as additional context
                error.insert(
                    ContextKind::ValidValue,
                    ContextValue::Strings(vec![
                        "b".into(),
                        "k, kb".into(),
                        "ki, kib".into(),
                        "m, mb".into(),
                        "mi, mib".into(),
                        "g, gb".into(),
                        "gi, gib".into(),
                        "t, tb".into(),
                        "ti, tib".into(),
                    ]),
                );

                Err(error)
            }
        }
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        // Provide examples but don't restrict to only these values (allow user to have custom entries but allows to use these as a template)
        Some(Box::new(
            [
                // No prefix - exact size
                PossibleValue::new("100").help("exactly 100 bytes"),
                PossibleValue::new("1k").help("exactly 1 kilobyte (1000 bytes)"),
                PossibleValue::new("1ki").help("exactly 1 kibibyte (1024 bytes)"),
                PossibleValue::new("10mb").help("exactly 10 megabytes"),
                PossibleValue::new("1gb").help("exactly 1 gigabyte"),
                // + prefix - larger than
                PossibleValue::new("+1m").help("larger than 1MB"),
                PossibleValue::new("+10mb").help("larger than 10MB"),
                PossibleValue::new("+1gib").help("larger than 1GiB"),
                // - prefix - smaller than
                PossibleValue::new("-500k").help("smaller than 500KB"),
                PossibleValue::new("-10mb").help("smaller than 10MB"),
                PossibleValue::new("-1gib").help("smaller than 1GiB"),
            ]
            .into_iter(),
        ))
    }
}

#[derive(Clone, Debug)]
#[allow(clippy::exhaustive_structs)]
/// A struct to provide completions for filetype completions in CLI
pub struct FileTypeParser;

impl TypedValueParser for FileTypeParser {
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

                //
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

                // all valid values
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
