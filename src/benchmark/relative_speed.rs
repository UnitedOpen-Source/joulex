use std::cmp::Ordering;

use super::benchmark_result::BenchmarkResult;
use crate::{metric::Metric, options::SortOrder, util::units::Scalar};

#[derive(Debug)]
pub struct BenchmarkResultWithRelativeSpeed<'a> {
    pub result: &'a BenchmarkResult,
    pub relative_speed: Scalar,
    pub relative_speed_stddev: Option<Scalar>,
    pub is_reference: bool,
    // Less means better (faster, less energy, less memory)
    pub relative_ordering: Ordering,
}

pub fn compare_metric(l: &BenchmarkResult, r: &BenchmarkResult, metric: Metric) -> Ordering {
    match (l.timed_out, r.timed_out) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => {
            let l_val = l.primary_mean(metric);
            let r_val = r.primary_mean(metric);
            l_val.partial_cmp(&r_val).unwrap_or(Ordering::Equal)
        }
    }
}

pub fn compare_mean_time(l: &BenchmarkResult, r: &BenchmarkResult) -> Ordering {
    compare_metric(l, r, Metric::Wall)
}

/// The result with the smallest primary metric value, excluding timed-out benchmarks if possible.
///
/// # Panics
/// If `results` is empty. Every caller checks this first.
pub fn best_of(results: &[BenchmarkResult], metric: Metric) -> &BenchmarkResult {
    results
        .iter()
        .filter(|r| !r.timed_out)
        .min_by(|&l, &r| compare_metric(l, r, metric))
        .or_else(|| results.iter().min_by(|&l, &r| compare_metric(l, r, metric)))
        .expect("at least one benchmark result")
}

pub fn fastest_of(results: &[BenchmarkResult]) -> &BenchmarkResult {
    best_of(results, Metric::Wall)
}

fn compute_relative_speeds<'a>(
    results: &'a [BenchmarkResult],
    reference: &'a BenchmarkResult,
    sort_order: SortOrder,
    metric: Metric,
) -> Vec<BenchmarkResultWithRelativeSpeed<'a>> {
    let ref_mean = reference.primary_mean(metric);
    let ref_stddev = reference.primary_stddev(metric);

    let mut results: Vec<_> = results
        .iter()
        .map(|result| {
            let is_reference = std::ptr::eq(result, reference);
            let relative_ordering = compare_metric(result, reference, metric);

            if result.timed_out {
                return BenchmarkResultWithRelativeSpeed {
                    result,
                    relative_speed: f64::NAN,
                    relative_speed_stddev: None,
                    is_reference,
                    relative_ordering: Ordering::Greater,
                };
            }

            let res_mean = result.primary_mean(metric);
            let res_stddev = result.primary_stddev(metric);

            if res_mean == 0.0 {
                return BenchmarkResultWithRelativeSpeed {
                    result,
                    relative_speed: if is_reference { 1.0 } else { f64::INFINITY },
                    relative_speed_stddev: None,
                    is_reference,
                    relative_ordering,
                };
            }

            let ratio = match relative_ordering {
                Ordering::Less => ref_mean / res_mean,
                Ordering::Equal => 1.0,
                Ordering::Greater => res_mean / ref_mean,
            };

            // https://en.wikipedia.org/wiki/Propagation_of_uncertainty#Example_formulas
            // Covariance assumed to be 0, i.e. variables are assumed to be independent
            let ratio_stddev = match (res_stddev, ref_stddev) {
                (Some(result_stddev), Some(fastest_stddev)) => Some(
                    ratio
                        * ((result_stddev / res_mean).powi(2)
                            + (fastest_stddev / ref_mean).powi(2))
                        .sqrt(),
                ),
                _ => None,
            };

            BenchmarkResultWithRelativeSpeed {
                result,
                relative_speed: ratio,
                relative_speed_stddev: ratio_stddev,
                is_reference,
                relative_ordering,
            }
        })
        .collect();

    match sort_order {
        SortOrder::Command => {}
        SortOrder::MeanTime => {
            results.sort_unstable_by(|r1, r2| compare_metric(r1.result, r2.result, metric));
        }
    }

    results
}

pub fn compute_with_check_from_reference<'a>(
    results: &'a [BenchmarkResult],
    reference: &'a BenchmarkResult,
    sort_order: SortOrder,
    metric: Metric,
) -> Option<Vec<BenchmarkResultWithRelativeSpeed<'a>>> {
    if results.is_empty() {
        return Some(Vec::new());
    }

    if best_of(results, metric).primary_mean(metric) == 0.0 || reference.primary_mean(metric) == 0.0
    {
        return None;
    }

    Some(compute_relative_speeds(
        results, reference, sort_order, metric,
    ))
}

pub fn compute_with_check<'a>(
    results: &'a [BenchmarkResult],
    sort_order: SortOrder,
    metric: Metric,
) -> Option<Vec<BenchmarkResultWithRelativeSpeed<'a>>> {
    if results.is_empty() {
        return Some(Vec::new());
    }

    let fastest = best_of(results, metric);

    if fastest.primary_mean(metric) == 0.0 {
        return None;
    }

    Some(compute_relative_speeds(
        results, fastest, sort_order, metric,
    ))
}

/// Fallback when relative speed cannot be computed (e.g. fastest is 0.0).
/// Populates entries with relative_speed = NaN and relative_speed_stddev = None.
pub fn compute_without_ratios<'a>(
    results: &'a [BenchmarkResult],
    reference: &'a BenchmarkResult,
    sort_order: SortOrder,
    metric: Metric,
) -> Vec<BenchmarkResultWithRelativeSpeed<'a>> {
    if results.is_empty() {
        return Vec::new();
    }

    let mut results: Vec<_> = results
        .iter()
        .map(|result| {
            let is_reference = std::ptr::eq(result, reference);
            let relative_ordering = compare_metric(result, reference, metric);

            BenchmarkResultWithRelativeSpeed {
                result,
                relative_speed: f64::NAN,
                relative_speed_stddev: None,
                is_reference,
                relative_ordering,
            }
        })
        .collect();

    match sort_order {
        SortOrder::Command => {}
        SortOrder::MeanTime => {
            results.sort_unstable_by(|r1, r2| compare_metric(r1.result, r2.result, metric));
        }
    }

    results
}

/// Same as compute_with_check, potentially resulting in relative speeds of infinity
pub fn compute<'a>(
    results: &'a [BenchmarkResult],
    sort_order: SortOrder,
    metric: Metric,
) -> Vec<BenchmarkResultWithRelativeSpeed<'a>> {
    if results.is_empty() {
        return Vec::new();
    }

    let fastest = best_of(results, metric);

    compute_relative_speeds(results, fastest, sort_order, metric)
}

#[cfg(test)]
fn create_result(name: &str, mean: Scalar) -> BenchmarkResult {
    use std::collections::BTreeMap;

    BenchmarkResult {
        command: name.into(),
        command_with_unused_parameters: name.into(),
        mean,
        stddev: Some(1.0),
        median: mean,
        user: mean,
        system: 0.0,
        cpu_percent: None,
        min: mean,
        max: mean,
        times: None,
        user_times: None,
        system_times: None,
        memory_usage_byte: None,
        mean_energy_joules: None,
        mean_watts: None,
        energy_joules: None,
        exit_codes: Vec::new(),
        parameters: BTreeMap::new(),
        omitted_failed_runs: Vec::new(),
        discarded_outliers: Vec::new(),
        resources: None,
        percentiles: None,
        geometric_mean: None,
        warmup_runs: None,
        runs_planned: None,
        first_run: None,
        diagnostics: None,
        per_run_parameters: None,
        precision: None,
        shell: None,
        baseline: None,
        ..Default::default()
    }
}

#[test]
fn test_compute_relative_speed() {
    use approx::assert_relative_eq;

    let results = vec![
        create_result("cmd1", 3.0),
        create_result("cmd2", 2.0),
        create_result("cmd3", 5.0),
    ];

    let annotated_results = compute_with_check(&results, SortOrder::Command, Metric::Wall).unwrap();

    assert_relative_eq!(1.5, annotated_results[0].relative_speed);
    assert_relative_eq!(1.0, annotated_results[1].relative_speed);
    assert_relative_eq!(2.5, annotated_results[2].relative_speed);
}

#[test]
fn test_compute_relative_speed_with_reference() {
    use approx::assert_relative_eq;

    let results = vec![create_result("cmd2", 2.0), create_result("cmd3", 5.0)];
    let reference = create_result("cmd2", 4.0);

    let annotated_results =
        compute_with_check_from_reference(&results, &reference, SortOrder::Command, Metric::Wall)
            .unwrap();

    assert_relative_eq!(2.0, annotated_results[0].relative_speed);
    assert_relative_eq!(1.25, annotated_results[1].relative_speed);
}

#[test]
fn test_compute_relative_speed_for_zero_times() {
    let results = vec![create_result("cmd1", 1.0), create_result("cmd2", 0.0)];

    let annotated_results = compute_with_check(&results, SortOrder::Command, Metric::Wall);

    assert!(annotated_results.is_none());
}

#[test]
fn test_compute_without_ratios() {
    let results = vec![create_result("cmd1", 0.0), create_result("cmd2", 0.0)];

    let annotated_results =
        compute_without_ratios(&results, &results[0], SortOrder::Command, Metric::Wall);
    assert_eq!(annotated_results.len(), 2);
    assert!(annotated_results[0].relative_speed.is_nan());
    assert!(annotated_results[1].relative_speed.is_nan());
    assert!(annotated_results[0].relative_speed_stddev.is_none());
    assert!(annotated_results[1].relative_speed_stddev.is_none());
    assert!(annotated_results[0].is_reference);
    assert!(!annotated_results[1].is_reference);
}

#[test]
fn test_compute_without_ratios_identical_results_single_reference() {
    let results = vec![create_result("cmd1", 0.0), create_result("cmd1", 0.0)];

    let annotated_results =
        compute_without_ratios(&results, &results[1], SortOrder::Command, Metric::Wall);
    assert!(!annotated_results[0].is_reference);
    assert!(annotated_results[1].is_reference);
}

#[test]
fn test_compute_relative_speed_identical_results_single_reference() {
    let results = vec![create_result("cmd1", 1.0), create_result("cmd1", 1.0)];

    let annotated_results = compute_with_check(&results, SortOrder::Command, Metric::Wall).unwrap();
    assert_eq!(
        annotated_results.iter().filter(|r| r.is_reference).count(),
        1
    );
}

#[test]
fn test_compute_empty_results() {
    let results: Vec<BenchmarkResult> = Vec::new();
    assert!(
        compute_with_check(&results, SortOrder::Command, Metric::Wall)
            .unwrap()
            .is_empty()
    );
    assert!(compute(&results, SortOrder::Command, Metric::Wall).is_empty());
}

#[test]
fn test_compute_relative_speed_energy_and_memory() {
    use approx::assert_relative_eq;

    let res1 = BenchmarkResult {
        command: "cmd1".into(),
        mean: 1.0,
        mean_energy_joules: Some(10.0),
        memory_usage_byte: Some(vec![1000]),
        ..Default::default()
    };
    let res2 = BenchmarkResult {
        command: "cmd2".into(),
        mean: 2.0,
        mean_energy_joules: Some(5.0),
        memory_usage_byte: Some(vec![3000]),
        ..Default::default()
    };
    let results = vec![res1, res2];

    // Under Metric::Wall: cmd1 is fastest (1.0 vs 2.0)
    let wall_speeds = compute_with_check(&results, SortOrder::Command, Metric::Wall).unwrap();
    assert_relative_eq!(1.0, wall_speeds[0].relative_speed);
    assert_relative_eq!(2.0, wall_speeds[1].relative_speed);

    // Under Metric::Energy: cmd2 uses less energy (5.0 vs 10.0) -> cmd2 is reference/best
    let energy_speeds = compute_with_check(&results, SortOrder::Command, Metric::Energy).unwrap();
    assert_relative_eq!(2.0, energy_speeds[0].relative_speed);
    assert_relative_eq!(1.0, energy_speeds[1].relative_speed);

    // Under Metric::Memory: cmd1 uses less memory (1000 vs 3000) -> cmd1 is best
    let mem_speeds = compute_with_check(&results, SortOrder::Command, Metric::Memory).unwrap();
    assert_relative_eq!(1.0, mem_speeds[0].relative_speed);
    assert_relative_eq!(3.0, mem_speeds[1].relative_speed);
}
