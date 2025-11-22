#![allow(clippy::missing_errors_doc)]
use clap::{
    Arg, Command, Error,
    builder::{PossibleValue, TypedValueParser},
    error::{ContextKind, ContextValue, ErrorKind},
};
use core::fmt;
use std::ffi::OsStr;

pub const KILO: u64 = 1000;
pub const MEGA: u64 = KILO * 1000;
pub const GIGA: u64 = MEGA * 1000;
pub const TERA: u64 = GIGA * 1000;

pub const KIBI: u64 = 1024;
pub const MEBI: u64 = KIBI * 1024;
pub const GIBI: u64 = MEBI * 1024;
pub const TEBI: u64 = GIBI * 1024;

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

    /// Checks if the given file size satisfies this size filter
    #[must_use]
    pub const fn is_within_size(&self, size: u64) -> bool {
        match *self {
            Self::Max(limit) => size <= limit,
            Self::Min(limit) => size >= limit,
            Self::Equals(limit) => size == limit,
        }
    }
}

/// A Custom parser that provides helpful error messages and suggestions for file sizes
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

                // Examples as suggestions - clearly showing + and - prefixes
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
