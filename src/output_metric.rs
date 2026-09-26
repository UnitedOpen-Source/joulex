use anyhow::{ensure, Result};
use regex::bytes::Regex;
use std::str::FromStr;

/// A user-specified metric to extract from a command's standard output.
#[derive(Debug, Clone)]
pub struct OutputMetric {
    pub name: String,
    pub re: Regex,
}

impl PartialEq for OutputMetric {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.re.as_str() == other.re.as_str()
    }
}

impl FromStr for OutputMetric {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let (name, pattern) = s
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("expected 'NAME=REGEX', got '{s}'"))?;

        ensure!(
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "invalid metric name '{name}': names must contain only ASCII alphanumeric characters and underscores"
        );

        let re =
            Regex::new(pattern).map_err(|e| anyhow::anyhow!("invalid regex '{pattern}': {e}"))?;

        ensure!(
            re.captures_len() >= 2,
            "regex for metric '{name}' must contain at least one capture group '(...)'"
        );

        Ok(Self {
            name: name.to_string(),
            re,
        })
    }
}

impl OutputMetric {
    /// Extracts the numeric metric value from the given output bytes.
    /// Returns the parsed f64 from the first capture group of the LAST match.
    pub fn extract(&self, output: &[u8]) -> Option<f64> {
        let caps = self.re.captures_iter(output).last()?;
        let matched_bytes = caps.get(1)?.as_bytes();
        let s = std::str::from_utf8(matched_bytes).ok()?;
        s.trim().parse::<f64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_metric_from_str_valid() {
        let m = OutputMetric::from_str("latency=([0-9.]+)ms").unwrap();
        assert_eq!(m.name, "latency");

        let m2 = OutputMetric::from_str("qps_total=Total QPS: ([0-9]+)").unwrap();
        assert_eq!(m2.name, "qps_total");
    }

    #[test]
    fn test_output_metric_from_str_invalid() {
        // No '='
        assert!(OutputMetric::from_str("latency([0-9]+)").is_err());

        // Empty name
        assert!(OutputMetric::from_str("=([0-9]+)").is_err());

        // Invalid name with hyphen
        assert!(OutputMetric::from_str("my-metric=([0-9]+)").is_err());

        // Invalid name with space
        assert!(OutputMetric::from_str("my metric=([0-9]+)").is_err());

        // No capture group
        assert!(OutputMetric::from_str("latency=[0-9]+").is_err());

        // Invalid regex syntax
        assert!(OutputMetric::from_str("latency=([0-9+").is_err());
    }

    #[test]
    fn test_output_metric_extract() {
        let m = OutputMetric::from_str("latency=time=([0-9.]+)s").unwrap();

        // Single match
        let out = b"setup... time=2.5s done";
        assert_eq!(m.extract(out), Some(2.5));

        // Multiple matches -> takes last match
        let out_multi = b"pass 1: time=1.0s\npass 2: time=4.75s\nall done";
        assert_eq!(m.extract(out_multi), Some(4.75));

        // Scientific notation
        let m_sci = OutputMetric::from_str("err=error: ([0-9.eE+-]+)").unwrap();
        assert_eq!(m_sci.extract(b"error: 1.5e-3"), Some(0.0015));

        // Non-numeric capture
        assert_eq!(m.extract(b"time=unknowns"), None);

        // No match
        assert_eq!(m.extract(b"no timings here"), None);
    }
}
