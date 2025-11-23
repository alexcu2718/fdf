use clap::{
    Arg, Command, Error,
    builder::{PossibleValue, TypedValueParser},
    error::{ContextKind, ContextValue, ErrorKind},
};
use core::time::Duration;
use std::{ffi::OsStr, fmt, time::SystemTime};

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
 use fdf::filters::TimeFilter;

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
     Parses a time string and returns a `TimeFilter`.

     # Arguments

     * `s` - A string slice containing the time specification

     # Format

     The expected format is: `[+|-]<number><unit>` or for between: `<number><unit>..<number><unit>`
     - `-` prefix: files modified within the last X time (After/newer than)
     - `+` prefix: files modified more than X time ago (Before/older than)
     - `..` separator: between two times (e.g., "2d..1d" = files modified between 2 days and 1 day ago)
     - Supported units: s (seconds), m (minutes), h (hours), d (days), w (weeks), y (years)

     # Examples

     ```
     use fdf::filters::TimeFilter;

     // Files modified within the last hour
     let filter = TimeFilter::from_string("-1h").unwrap();

     // Files modified more than 2 days ago
     let filter = TimeFilter::from_string("+2d").unwrap();

     // Files modified between 2 days and 1 day ago
     let filter = TimeFilter::from_string("2d..1d").unwrap();
     ```

     # Errors

     Returns `ParseTimeError::InvalidFormat` if the string cannot be parsed or is in an invalid format.
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

/// A Custom parser that provides helpful error messages and suggestions for filtering by time modified
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

                // Main error
                error.insert(
                    ContextKind::InvalidValue,
                    ContextValue::String(format!("{err}")),
                );

                // Examples as suggestions
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
