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

    /// 5th, 25th, 75th and 95th percentile of the run times (linear
    /// interpolation between closest ranks)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percentiles: Option<Percentiles>,

    /// Number of warmup runs performed by `--warmup auto`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warmup_runs: Option<u64>,

    /// Geometric mean of the run times (omitted if a run took 0 s)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometric_mean: Option<Second>,

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

    /// Per-run OS resource counters (context switches, page faults, block
    /// I/O), exported with --resource-usage on Unix
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<super::timing_result::ResourceSeries>,

    /// Number of planned runs if the benchmark was interrupted
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs_planned: Option<u64>,

    /// The first (cold) run, reported separately with `--first-run=separate`.
    /// It is not part of any other field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_run: Option<FirstRun>,

    /// Interference diagnostics of the run times (trend, multimodality,
    /// outlier-inflated variance)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<crate::stats::diagnostics::Diagnostics>,

    /// Which per-run parameter values each run used (`--parameter-sample`,
    /// `--aggregate-parameter-runs`), and the statistics per value
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_run_parameters: Option<PerRunParameterValues>,
}

/// Per-run parameter values, aligned with `times`, and statistics per value.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerRunParameterValues {
    /// For each variable, the value of every run (aligned with `times`)
    pub values: BTreeMap<String, Vec<String>>,
    /// For each variable and value: statistics of the runs that used it
    pub stats: BTreeMap<String, BTreeMap<String, ValueStats>>,
}

/// Run time statistics of the runs that used one parameter value.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValueStats {
    pub mean: Second,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stddev: Option<Second>,
    pub runs: usize,
}

impl PerRunParameterValues {
    /// From the values of each run (as (name, value) pairs) and the run times.
    pub fn new(per_run: &[Vec<(String, String)>], times: &[Second]) -> Option<Self> {
        if per_run.is_empty() || per_run.iter().all(Vec::is_empty) || per_run.len() != times.len() {
            return None;
        }
        let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut by_value: BTreeMap<String, BTreeMap<String, Vec<Second>>> = BTreeMap::new();
        for (run, &time) in per_run.iter().zip(times) {
            for (name, value) in run {
                values.entry(name.clone()).or_default().push(value.clone());
                by_value
                    .entry(name.clone())
                    .or_default()
                    .entry(value.clone())
                    .or_default()
                    .push(time);
            }
        }
        let stats = by_value
            .into_iter()
            .map(|(name, groups)| {
                let groups = groups
                    .into_iter()
                    .map(|(value, times)| {
                        let mean = crate::stats::basic::mean(&times);
                        let stddev = (times.len() > 1)
                            .then(|| crate::stats::basic::standard_deviation(&times, Some(mean)));
                        (
                            value,
                            ValueStats {
                                mean,
                                stddev,
                                runs: times.len(),
                            },
                        )
                    })
                    .collect();
                (name, groups)
            })
            .collect();
        Some(PerRunParameterValues { values, stats })
    }
}

/// The first (cold) run of a benchmark (`--first-run=separate`).
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct FirstRun {
    /// Wall-clock time in seconds
    pub time: Second,
    /// User and system time in seconds (not available for a warmup run)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<Second>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<Second>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_usage_byte: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_joules: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// True if this was the first warmup run (`--warmup` was used)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub warmup: bool,
}

/// Percentiles of the run times, in seconds.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Percentiles {
    pub p05: Second,
    pub p25: Second,
    pub p75: Second,
    pub p95: Second,
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

#[test]
fn per_run_parameter_values_and_stats() {
    let run = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect()
    };
    let per_run = [
        run(&[("f", "a"), ("t", "1")]),
        run(&[("f", "b"), ("t", "1")]),
        run(&[("f", "a"), ("t", "2")]),
    ];
    let values = PerRunParameterValues::new(&per_run, &[1.0, 2.0, 3.0]).unwrap();
    assert_eq!(values.values["f"], ["a", "b", "a"]);
    assert_eq!(values.values["t"], ["1", "1", "2"]);
    let a = &values.stats["f"]["a"];
    assert_eq!((a.mean, a.runs), (2.0, 2));
    assert!((a.stddev.unwrap() - std::f64::consts::SQRT_2).abs() < 1e-12);
    assert_eq!(values.stats["f"]["b"].stddev, None);

    // Nothing without per-run parameters, or if misaligned
    assert_eq!(
        PerRunParameterValues::new(&[vec![], vec![]], &[1.0, 2.0]),
        None
    );
    assert_eq!(PerRunParameterValues::new(&per_run, &[1.0]), None);
}
