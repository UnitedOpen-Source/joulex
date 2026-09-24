use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::util::units::Second;

/// Set of values that will be exported.
// NOTE: `serde` is used for JSON serialization, but not for CSV serialization due to the
// `parameters` map. Update `src/hyperfine/export/csv.rs` with new fields, as appropriate.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkResult {
    /// The full command line of the program that is being benchmarked
    pub command: String,

    /// The full command line of the program that is being benchmarked, possibly including a list of
    /// parameters that were not used in the command line template.
    #[serde(default, skip_serializing)]
    pub command_with_unused_parameters: String,

    /// The average run time
    pub mean: Second,

    /// The standard deviation of all run times. Not available if only one run has been performed
    pub stddev: Option<Second>,

    /// The median run time
    pub median: Second,

    /// Time spent in user mode
    pub user: Second,

    /// Time spent in kernel mode
    pub system: Second,

    /// CPU utilization percentage: (user + system) / mean * 100.0
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,

    /// Minimum of all measured times
    pub min: Second,

    /// Maximum of all measured times
    pub max: Second,

    /// All run time measurements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub times: Option<Vec<Second>>,

    /// User CPU time measurements for each run
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_times: Option<Vec<Second>>,

    /// System/kernel CPU time measurements for each run
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_times: Option<Vec<Second>>,

    /// Maximum memory usage of the process, in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage_byte: Option<Vec<u64>>,

    /// Average energy consumed in Joules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean_energy_joules: Option<f64>,

    /// Average power consumption in Watts (mean_energy_joules / mean_time)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean_watts: Option<f64>,

    /// All energy measurements in Joules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub energy_joules: Option<Vec<f64>>,

    /// Exit codes of all command invocations
    #[serde(default)]
    pub exit_codes: Vec<Option<i32>>,

    /// Parameter values for this benchmark
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, String>,

    /// Benchmark runs omitted due to --omit-failed-runs
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted_failed_runs: Vec<OmittedRun>,

    /// Zero-based indices of the runs excluded from the statistics by
    /// --discard-outliers
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub discarded_outliers: Vec<usize>,
}

/// Information about a benchmark run that was omitted due to failure.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmittedRun {
    /// Zero-based index of the benchmark run
    pub index: usize,
    /// The exit code of the failed command, or None if terminated by signal
    pub exit_code: Option<i32>,
}

impl BenchmarkResult {
    /// Returns true if any run had a non-zero or missing exit code.
    pub fn has_failure(&self) -> bool {
        self.exit_codes.iter().any(|code| *code != Some(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result_with_exit_codes(codes: Vec<Option<i32>>) -> BenchmarkResult {
        BenchmarkResult {
            exit_codes: codes,
            ..Default::default()
        }
    }

    #[test]
    fn test_has_failure_all_success() {
        let r = result_with_exit_codes(vec![Some(0), Some(0), Some(0)]);
        assert!(!r.has_failure());
    }

    #[test]
    fn test_has_failure_with_nonzero() {
        let r = result_with_exit_codes(vec![Some(0), Some(1), Some(0)]);
        assert!(r.has_failure());
    }

    #[test]
    fn test_has_failure_with_signal_kill() {
        let r = result_with_exit_codes(vec![Some(0), None]);
        assert!(r.has_failure());
    }

    #[test]
    fn test_has_failure_empty() {
        let r = result_with_exit_codes(vec![]);
        assert!(!r.has_failure());
    }

    #[test]
    fn test_omitted_failed_runs_serde() {
        let r = BenchmarkResult {
            command: "cmd".into(),
            times: Some(vec![0.1, 0.2]),
            exit_codes: vec![Some(0), Some(0)],
            omitted_failed_runs: vec![OmittedRun {
                index: 1,
                exit_code: Some(42),
            }],
            ..Default::default()
        };
        assert!(!r.has_failure());

        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"omitted_failed_runs\":[{\"index\":1,\"exit_code\":42}]"));

        let deserialized: BenchmarkResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, r);
        assert!(!deserialized.has_failure());
    }

    #[test]
    fn test_omitted_failed_runs_empty_skips_serializing() {
        let r = BenchmarkResult {
            command: "cmd".into(),
            exit_codes: vec![Some(0)],
            ..Default::default()
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("omitted_failed_runs"));
    }
}
