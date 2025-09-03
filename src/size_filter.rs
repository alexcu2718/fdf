pub(crate) const KILO: u64 = 1000;
pub(crate) const MEGA: u64 = KILO * 1000;
pub(crate) const GIGA: u64 = MEGA * 1000;
pub(crate) const TERA: u64 = GIGA * 1000;

pub(crate) const KIBI: u64 = 1024;
pub(crate) const MEBI: u64 = KIBI * 1024;
pub(crate) const GIBI: u64 = MEBI * 1024;
pub(crate) const TEBI: u64 = GIBI * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_enums)]
pub enum ParseSizeError {
    Empty,
    InvalidNumber,
    InvalidUnit,
    InvalidFormat,
}

impl core::fmt::Display for ParseSizeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::Empty => write!(f, "empty size string"),
            Self::InvalidNumber => write!(f, "invalid number"),
            Self::InvalidUnit => write!(f, "invalid unit"),
            Self::InvalidFormat => write!(f, "invalid format"),
        }
    }
}

impl core::error::Error for ParseSizeError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::exhaustive_enums)]
pub enum SizeFilter {
    Max(u64),
    Min(u64),
    Equals(u64),
}

impl SizeFilter {
    #[allow(clippy::missing_errors_doc)] //private function not doing this
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
