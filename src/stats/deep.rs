//! Bootstrap confidence intervals and a two-sample bootstrap test for
//! `--deep-stats`.
//!
//! This replaces the unmaintained `criterion-stats` crate (#31) with the same
//! methods: percentile bootstrap intervals, and a Welch t statistic compared
//! against its distribution under the null hypothesis, obtained by resampling
//! the pooled ("mixed") samples. A fixed seed makes the results reproducible:
//! the same measurements always give the same intervals and p-value.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::stats::basic::{mean, median, standard_deviation};
use crate::stats::summary::percentile;

/// Number of bootstrap resamples
const NRESAMPLES: usize = 5000;
/// Confidence level of the intervals
const CONFIDENCE_LEVEL: f64 = 0.95;
/// Seed of the resampling RNG (fixed, for reproducible output)
const SEED: u64 = 0x7065_7266_7261_7469; // "perfrati"

#[derive(Debug, Clone, PartialEq)]
pub struct DeepStats {
    pub mean_ci_lower: f64,
    pub mean_ci_upper: f64,
    pub median_ci_lower: f64,
    pub median_ci_upper: f64,
    pub std_dev_ci_lower: f64,
    pub std_dev_ci_upper: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComparisonStats {
    pub p_value: f64,
    pub t_statistic: f64,
    pub is_significant_05: bool,
    pub is_significant_01: bool,
}

/// Draw `n` values from `pool` with replacement into `out`.
fn resample_into(rng: &mut StdRng, pool: &[f64], n: usize, out: &mut Vec<f64>) {
    out.clear();
    out.extend((0..n).map(|_| pool[rng.gen_range(0..pool.len())]));
}

/// Lower and upper bound of the central `CONFIDENCE_LEVEL` interval of a
/// bootstrap distribution (percentile method).
fn confidence_interval(mut distribution: Vec<f64>) -> (f64, f64) {
    distribution.sort_by(f64::total_cmp);
    let tail = 50.0 * (1.0 - CONFIDENCE_LEVEL);
    (
        percentile(&distribution, tail).unwrap_or(f64::NAN),
        percentile(&distribution, 100.0 - tail).unwrap_or(f64::NAN),
    )
}

/// Welch's t statistic (sample variances, n - 1), incorporating optional baseline
/// mean variances from `--subtract`.
fn welch_t_with_variance(a: &[f64], b: &[f64], var_base_a: f64, var_base_b: f64) -> f64 {
    let (mean_a, mean_b) = (mean(a), mean(b));
    let var_a = standard_deviation(a, Some(mean_a)).powi(2);
    let var_b = standard_deviation(b, Some(mean_b)).powi(2);
    let se_sq = var_a / a.len() as f64 + var_base_a + var_b / b.len() as f64 + var_base_b;
    (mean_a - mean_b) / se_sq.sqrt()
}

/// Welch's t statistic (sample variances, n - 1).
#[cfg(test)]
fn welch_t(a: &[f64], b: &[f64]) -> f64 {
    welch_t_with_variance(a, b, 0.0, 0.0)
}

/// Two-tailed p-value of `t` in the null distribution (the same definition as
/// criterion-stats: twice the smaller tail fraction).
fn two_tailed_p_value(distribution: &[f64], t: f64) -> f64 {
    let n = distribution.len();
    let below = distribution.iter().filter(|&&x| x < t).count();
    below.min(n - below) as f64 / n as f64 * 2.0
}

/// Compute 95% bootstrap confidence intervals for mean, median, and std_dev
pub fn compute_deep_stats(data: &[f64]) -> Option<DeepStats> {
    if data.len() < 3 {
        return None;
    }

    let mut rng = StdRng::seed_from_u64(SEED);
    let mut resample = Vec::with_capacity(data.len());
    let mut means = Vec::with_capacity(NRESAMPLES);
    let mut medians = Vec::with_capacity(NRESAMPLES);
    let mut std_devs = Vec::with_capacity(NRESAMPLES);

    for _ in 0..NRESAMPLES {
        resample_into(&mut rng, data, data.len(), &mut resample);
        let m = mean(&resample);
        means.push(m);
        medians.push(median(&resample));
        std_devs.push(standard_deviation(&resample, Some(m)));
    }

    let (mean_ci_lower, mean_ci_upper) = confidence_interval(means);
    let (median_ci_lower, median_ci_upper) = confidence_interval(medians);
    let (std_dev_ci_lower, std_dev_ci_upper) = confidence_interval(std_devs);

    Some(DeepStats {
        mean_ci_lower,
        mean_ci_upper,
        median_ci_lower,
        median_ci_upper,
        std_dev_ci_lower,
        std_dev_ci_upper,
    })
}

/// Perform a two-sample bootstrap hypothesis test comparing sample `a` and sample `b`,
/// incorporating optional baseline mean variances from `--subtract`.
pub fn compare_samples_with_baseline_variance(
    a: &[f64],
    b: &[f64],
    var_base_a: f64,
    var_base_b: f64,
) -> Option<ComparisonStats> {
    if a.len() < 3 || b.len() < 3 {
        return None;
    }

    let t_stat = welch_t_with_variance(a, b, var_base_a, var_base_b);
    if !t_stat.is_finite() {
        // Zero variance in both samples:
        // If means are equal, the samples are identical -> p = 1.0, no difference
        let same = (mean(a) - mean(b)).abs() <= f64::EPSILON * mean(a).abs().max(1.0);
        return if same {
            Some(ComparisonStats {
                p_value: 1.0,
                t_statistic: 0.0,
                is_significant_05: false,
                is_significant_01: false,
            })
        } else {
            // Constant but different samples without variance: t-test is undefined
            None
        };
    }

    // Null distribution of t: resample both groups from the pooled sample.
    let pooled: Vec<f64> = a.iter().chain(b).copied().collect();
    let mut rng = StdRng::seed_from_u64(SEED);
    let (mut resample_a, mut resample_b) = (Vec::new(), Vec::new());
    let distribution: Vec<f64> = (0..NRESAMPLES)
        .map(|_| {
            resample_into(&mut rng, &pooled, a.len(), &mut resample_a);
            resample_into(&mut rng, &pooled, b.len(), &mut resample_b);
            welch_t_with_variance(&resample_a, &resample_b, var_base_a, var_base_b)
        })
        .collect();
    let p_val = two_tailed_p_value(&distribution, t_stat);

    if !p_val.is_finite() {
        return None;
    }

    Some(ComparisonStats {
        p_value: p_val,
        t_statistic: t_stat,
        is_significant_05: p_val < 0.05,
        is_significant_01: p_val < 0.01,
    })
}

/// Perform a two-sample bootstrap hypothesis test comparing sample `a` and sample `b`
pub fn compare_samples(a: &[f64], b: &[f64]) -> Option<ComparisonStats> {
    compare_samples_with_baseline_variance(a, b, 0.0, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_deep_stats() {
        let data = vec![0.10, 0.11, 0.10, 0.12, 0.09, 0.11];
        let stats = compute_deep_stats(&data).unwrap();
        assert!(stats.mean_ci_lower <= stats.mean_ci_upper);
        assert!(stats.median_ci_lower <= stats.median_ci_upper);
        assert!(stats.std_dev_ci_lower <= stats.std_dev_ci_upper);
    }

    #[test]
    fn test_compare_samples_significance() {
        let sample_fast = vec![0.01, 0.011, 0.01, 0.012, 0.009, 0.011];
        let sample_slow = vec![0.50, 0.51, 0.49, 0.52, 0.48, 0.51];
        let cmp = compare_samples(&sample_fast, &sample_slow).unwrap();
        assert!(cmp.is_significant_05);
        assert!(cmp.is_significant_01);
    }

    #[test]
    fn test_compare_samples_identical_constant() {
        let sample1 = vec![1.0, 1.0, 1.0, 1.0];
        let sample2 = vec![1.0, 1.0, 1.0, 1.0];
        let cmp = compare_samples(&sample1, &sample2).unwrap();
        assert_eq!(cmp.p_value, 1.0);
        assert_eq!(cmp.t_statistic, 0.0);
        assert!(!cmp.is_significant_05);
        assert!(!cmp.is_significant_01);
    }

    #[test]
    fn test_compare_samples_different_constant() {
        let sample1 = vec![1.0, 1.0, 1.0, 1.0];
        let sample2 = vec![2.0, 2.0, 2.0, 2.0];
        assert!(compare_samples(&sample1, &sample2).is_none());
    }

    #[test]
    fn test_compare_samples_insufficient() {
        let sample1 = vec![1.0, 1.0];
        let sample2 = vec![1.0, 1.0];
        assert!(compare_samples(&sample1, &sample2).is_none());
    }

    #[test]
    fn test_welch_t_matches_the_textbook_formula() {
        // means 2 and 5, sample variances 1 and 1, n = 3 each
        let t = welch_t(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]);
        assert!((t - (-3.0 / (2.0f64 / 3.0).sqrt())).abs() < 1e-12);
    }

    #[test]
    fn test_two_tailed_p_value() {
        let distribution: Vec<f64> = (0..100).map(f64::from).collect();
        assert_eq!(two_tailed_p_value(&distribution, 50.0), 1.0);
        assert_eq!(two_tailed_p_value(&distribution, 5.0), 0.1);
        assert_eq!(two_tailed_p_value(&distribution, 1000.0), 0.0);
    }

    #[test]
    fn test_confidence_intervals_contain_the_point_estimates() {
        let data: Vec<f64> = (0..40).map(|i| 1.0 + f64::from(i % 7) * 0.01).collect();
        let stats = compute_deep_stats(&data).unwrap();
        let m = mean(&data);
        assert!(stats.mean_ci_lower <= m && m <= stats.mean_ci_upper);
        let md = median(&data);
        assert!(stats.median_ci_lower <= md && md <= stats.median_ci_upper);
        let sd = standard_deviation(&data, None);
        assert!(stats.std_dev_ci_lower <= sd && sd <= stats.std_dev_ci_upper);
    }

    #[test]
    fn test_results_are_reproducible() {
        let data = vec![0.10, 0.11, 0.10, 0.12, 0.09, 0.11, 0.13, 0.10];
        assert_eq!(compute_deep_stats(&data), compute_deep_stats(&data));
        let other = vec![0.12, 0.13, 0.11, 0.14, 0.12, 0.13, 0.12, 0.15];
        assert_eq!(
            compare_samples(&data, &other),
            compare_samples(&data, &other)
        );
    }

    #[test]
    fn test_no_significance_for_samples_from_the_same_distribution() {
        let a = vec![0.100, 0.102, 0.099, 0.101, 0.100, 0.103, 0.098, 0.101];
        let b = vec![0.101, 0.099, 0.100, 0.102, 0.100, 0.098, 0.101, 0.100];
        let cmp = compare_samples(&a, &b).unwrap();
        assert!(!cmp.is_significant_05, "{cmp:?}");
    }

    #[test]
    fn test_baseline_variance_increases_p_value() {
        let a = vec![0.100, 0.102, 0.101, 0.103, 0.100];
        let b = vec![0.105, 0.107, 0.106, 0.108, 0.105];
        let without_base = compare_samples(&a, &b).unwrap();
        // With a noisy baseline subtracted, the difference is less certain
        let with_base = compare_samples_with_baseline_variance(&a, &b, 0.0001, 0.0001).unwrap();
        assert!(with_base.p_value >= without_base.p_value);
        assert!(with_base.t_statistic.abs() < without_base.t_statistic.abs());
    }
}
