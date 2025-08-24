pub(crate) const KILO: u64 = 1000;
pub(crate) const MEGA: u64 = KILO * 1000;
pub(crate) const GIGA: u64 = MEGA * 1000;
pub(crate) const TERA: u64 = GIGA * 1000;

pub(crate) const KIBI: u64 = 1024;
pub(crate) const MEBI: u64 = KIBI * 1024;
pub(crate) const GIBI: u64 = MEBI * 1024;
pub(crate) const TEBI: u64 = GIBI * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum ParseSizeError {
    Empty,
    InvalidNumber,
    InvalidUnit,
    InvalidFormat,
}

impl std::fmt::Display for ParseSizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseSizeError::Empty => write!(f, "empty size string"),
            ParseSizeError::InvalidNumber => write!(f, "invalid number"),
            ParseSizeError::InvalidUnit => write!(f, "invalid unit"),
            ParseSizeError::InvalidFormat => write!(f, "invalid format"),
        }
    }
}

impl std::error::Error for ParseSizeError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeFilter {
    Max(u64),
    Min(u64),
    Equals(u64),
}

impl SizeFilter {
    pub fn from_string(s: &str) -> Result<Self, ParseSizeError> {
        Self::parse_args(s).ok_or(ParseSizeError::InvalidFormat)
    }

    fn parse_args(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        // Determine prefix (+, -, or none)
        let (limit, remaining) = if s.starts_with('+') {
            ("+", &s[1..])
        } else if s.starts_with('-') {
            ("-", &s[1..])
        } else {
            ("", s)
        };

        let (quantity, unit_str) = Self::parse_size_parts(remaining)?;

      
        let multiplier = Self::unit_multiplier(&unit_str)?;

        let size = quantity * multiplier;
        match limit {
            "+" => Some(SizeFilter::Min(size)),
            "-" => Some(SizeFilter::Max(size)),
            "" => Some(SizeFilter::Equals(size)),
            _ => None,
        }
    }

    fn parse_size_parts(s: &str) -> Option<(u64, String)> {
        let s = s.trim().to_lowercase();
        let ref_s = s.as_str();

        // check where digits end
        let digit_end = ref_s
            .chars()
            .position(|c| !c.is_ascii_digit())
            .unwrap_or(s.len());

        if digit_end == s.len() {
            let quantity = s.parse().ok()?;
            return Some((quantity, "b".into()));
        }

        let (num_str, unit_str) = ref_s.split_at(digit_end);
        let quantity = num_str.parse().ok()?;

        Some((quantity, unit_str.into()))
    }

    fn unit_multiplier(unit: &str) -> Option<u64> {
        let unit = unit.trim().to_lowercase();
        match unit.as_ref() {
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

    pub fn is_within_size(&self, size: u64) -> bool {
        match *self {
            SizeFilter::Max(limit) => size <= limit,
            SizeFilter::Min(limit) => size >= limit,
            SizeFilter::Equals(limit) => size == limit,
        }
    }
}
