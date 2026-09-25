//! This module contains common units.

pub type Scalar = f64;

/// Type alias for unit of time
pub type Second = Scalar;

/// Supported time units
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Second,
    MilliSecond,
    MicroSecond,
}

impl Unit {
    /// The abbreviation of the Unit.
    pub fn short_name(self) -> String {
        match self {
            Unit::Second => String::from("s"),
            Unit::MilliSecond => String::from("ms"),
            Unit::MicroSecond => String::from("µs"),
        }
    }

    /// Returns the Second value formatted for the Unit, with the decimals of
    /// `--precision` (default: 3 for seconds, 1 for ms and µs).
    pub fn format(self, value: Second) -> String {
        let decimals = match precision() {
            Precision::Fixed(decimals) => decimals,
            Precision::Default | Precision::Auto => self.default_decimals(),
        };
        self.format_decimals(value, decimals)
    }

    /// The Second value in this unit, with the given number of decimals.
    pub fn format_decimals(self, value: Second, decimals: usize) -> String {
        format!("{:.decimals$}", value * self.per_second())
    }

    /// How many of this unit make one second.
    pub fn per_second(self) -> f64 {
        match self {
            Unit::Second => 1.0,
            Unit::MilliSecond => 1e3,
            Unit::MicroSecond => 1e6,
        }
    }

    fn default_decimals(self) -> usize {
        match self {
            Unit::Second => 3,
            Unit::MilliSecond | Unit::MicroSecond => 1,
        }
    }
}

/// `--precision`: number of decimals in human-readable output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precision {
    /// 3 decimals for seconds, 1 for milliseconds and microseconds
    #[default]
    Default,
    /// A fixed number of decimals
    Fixed(usize),
    /// Mean and σ with the decimals that keep two significant digits of σ
    Auto,
}

static PRECISION: std::sync::OnceLock<Precision> = std::sync::OnceLock::new();

/// Set the precision of human-readable output, once, at startup.
pub fn set_precision(precision: Precision) {
    let _ = PRECISION.set(precision);
}

pub fn precision() -> Precision {
    PRECISION.get().copied().unwrap_or_default()
}

/// Decimals that keep two significant digits of `stddev` (already converted to
/// the display unit), e.g. 1.488 → 1 ("1.5"), 0.041 → 3 ("0.041"), 23.4 → 0.
/// `None` if σ is zero or not finite.
pub fn auto_decimals(stddev: f64) -> Option<usize> {
    if !(stddev.is_finite() && stddev > 0.0) {
        return None;
    }
    let exponent = stddev.log10().floor() as i32;
    Some((1 - exponent).clamp(0, 9) as usize)
}

/// Format a byte count into a human-readable string (B, KB, MB, GB).
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Parse a human duration string (e.g. "500ms", "2s", "1m", "1.5s", "200µs", "200us") into `std::time::Duration`.
pub fn parse_duration(s: &str) -> Result<std::time::Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("duration cannot be empty".to_string());
    }

    // Split numeric prefix from unit suffix
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '+' && c != '-')
        .unwrap_or(s.len());

    let (num_str, unit_str) = s.split_at(num_end);
    let val: f64 = num_str
        .parse()
        .map_err(|e| format!("invalid number in duration: {e}"))?;

    if !val.is_finite() || val <= 0.0 {
        return Err("duration must be a positive number".to_string());
    }

    let multiplier = match unit_str.trim() {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
        "ms" | "msec" | "milli" | "millisecond" | "milliseconds" => 1e-3,
        "us" | "µs" | "usec" | "micro" | "microsecond" | "microseconds" => 1e-6,
        "ns" | "nsec" | "nanosecond" | "nanoseconds" => 1e-9,
        "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3600.0,
        unknown => return Err(format!("unknown duration unit '{unknown}'")),
    };

    let total_secs = val * multiplier;
    if !total_secs.is_finite() || total_secs <= 0.0 {
        return Err("duration must be a positive number".to_string());
    }

    Ok(std::time::Duration::from_secs_f64(total_secs))
}

#[test]
fn test_unit_short_name() {
    assert_eq!("s", Unit::Second.short_name());
    assert_eq!("ms", Unit::MilliSecond.short_name());
    assert_eq!("µs", Unit::MicroSecond.short_name());
}

// Note - the values are rounded when formatted.
#[test]
fn test_unit_format() {
    let value: Second = 123.456789;
    assert_eq!("123.457", Unit::Second.format(value));
    assert_eq!("123456.8", Unit::MilliSecond.format(value));

    assert_eq!("1234.6", Unit::MicroSecond.format(0.00123456));
}

#[test]
fn test_format_bytes() {
    assert_eq!("512 B", format_bytes(512));
    assert_eq!("1.5 KB", format_bytes(1536));
    assert_eq!("20.0 MB", format_bytes(20 * 1024 * 1024));
    assert_eq!(
        "1.50 GB",
        format_bytes((1.5 * 1024.0 * 1024.0 * 1024.0) as u64)
    );
}

#[test]
fn test_parse_duration() {
    use std::time::Duration;

    assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
    assert_eq!(parse_duration("2s").unwrap(), Duration::from_secs(2));
    assert_eq!(parse_duration("1m").unwrap(), Duration::from_secs(60));
    assert_eq!(parse_duration("1.5s").unwrap(), Duration::from_millis(1500));
    assert_eq!(parse_duration("200µs").unwrap(), Duration::from_micros(200));
    assert_eq!(parse_duration("200us").unwrap(), Duration::from_micros(200));
    assert_eq!(parse_duration("2").unwrap(), Duration::from_secs(2));
    assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));

    assert!(parse_duration("").is_err());
    assert!(parse_duration("0").is_err());
    assert!(parse_duration("0s").is_err());
    assert!(parse_duration("-2s").is_err());
    assert!(parse_duration("inf").is_err());
    assert!(parse_duration("nan").is_err());
    assert!(parse_duration("2xyz").is_err());
}
