//! Interference diagnostics on the run times of one benchmark (in run order):
//! a systematic trend (thermal throttling, a filling cache, a background job),
//! a multimodal distribution (two clusters of run times), and a variance that
//! is mostly caused by a few outliers.

use serde::{Deserialize, Serialize};

use crate::outlier_detection::modified_zscores;

/// Minimum number of runs for the trend test
pub const MIN_TREND_SAMPLES: usize = 8;
/// A trend is reported if the run time changes by at least this fraction
/// from the first to the last run ...
pub const TREND_MIN_REL_CHANGE: f64 = 0.05;
/// ... and the Mann–Kendall test is significant at this level. This is
/// stricter than the usual 0.01 because run times are rarely independent
/// (bursts of background load), which makes the test anticonservative.
pub const TREND_MAX_P: f64 = 0.001;
/// The trend test is O(n²); longer series are thinned evenly to this length.
const MAX_TREND_SAMPLES: usize = 1000;

/// Minimum number of runs for the multimodality check
pub const MIN_BIMODALITY_SAMPLES: usize = 30;
/// Sarle's bimodality coefficient above 5/9 suggests bi- or multimodality
pub const BIMODALITY_THRESHOLD: f64 = 5.0 / 9.0;
/// Each of two modes must contain at least this fraction of the runs ...
const MIN_MODE_FRACTION: f64 = 0.1;
/// ... and the density between them must drop below this fraction of the
/// lower peak.
const MAX_VALLEY_RATIO: f64 = 0.5;
/// Number of points at which the density estimate is evaluated
const KDE_GRID_POINTS: usize = 256;

/// Report outliers if they cause at least this fraction of the variance
pub const INFLATED_VARIANCE_FRACTION: f64 = 0.5;

/// Diagnostics of one benchmark, exported as `diagnostics` in the JSON.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Diagnostics {
    /// Relative change of the run time from the first to the last run
    /// (Theil–Sen slope), e.g. 0.084 for +8.4%
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trend_rel: Option<f64>,

    /// Two-sided p-value of the Mann–Kendall trend test
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trend_p: Option<f64>,

    /// Sarle's bimodality coefficient of the run times
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bimodality: Option<f64>,

    /// Whether a kernel density estimate of the run times has at least two
    /// separated modes (each with at least 10% of the runs,
    /// the density between them below half of the lower peak)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub separated_modes: bool,

    /// Fraction of the variance that disappears when outliers are removed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outlier_variance_fraction: Option<f64>,

    /// Number of outliers behind `outlier_variance_fraction`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outlier_count: Option<usize>,

    /// Number of runs the diagnostics were computed from
    #[serde(skip)]
    runs: usize,
}

impl Diagnostics {
    /// Compute all diagnostics for `times` (in run order). Outliers are runs
    /// with a modified Z-score above `outlier_threshold`.
    pub fn compute(times: &[f64], outlier_threshold: f64) -> Self {
        let mut diagnostics = Diagnostics {
            runs: times.len(),
            ..Default::default()
        };

        if let Some(trend) = trend(times) {
            diagnostics.trend_rel = Some(trend.rel_change);
            diagnostics.trend_p = Some(trend.p_value);
        }

        let scores = modified_zscores(times);
        let kept: Vec<f64> = times
            .iter()
            .zip(&scores)
            .filter(|(_, z)| z.abs() <= outlier_threshold)
            .map(|(&t, _)| t)
            .collect();

        // All runs: a minority cluster of run times has large modified
        // Z-scores and would otherwise be removed as "outliers". Isolated
        // outliers can't form a mode (each needs 10% of the runs).
        diagnostics.bimodality = bimodality_coefficient(times);
        diagnostics.separated_modes = has_separated_modes(times);

        if kept.len() < times.len() {
            diagnostics.outlier_variance_fraction = removed_variance_fraction(times, &kept);
            diagnostics.outlier_count = diagnostics
                .outlier_variance_fraction
                .map(|_| times.len() - kept.len());
        }

        diagnostics
    }

    /// The run times drift significantly and substantially over the runs.
    pub fn has_trend(&self) -> bool {
        matches!(
            (self.trend_rel, self.trend_p),
            (Some(rel), Some(p)) if rel.abs() >= TREND_MIN_REL_CHANGE && p < TREND_MAX_P
        )
    }

    /// The run time distribution has two (or more) clearly separated modes.
    pub fn is_multimodal(&self) -> bool {
        self.separated_modes && self.bimodality.is_some_and(|bc| bc > BIMODALITY_THRESHOLD)
    }

    /// Most of the variance is caused by a few outliers. "A few" means fewer
    /// than 10% of the runs; more are a cluster (see `is_multimodal`).
    pub fn has_inflated_variance(&self) -> bool {
        self.outlier_variance_fraction
            .is_some_and(|f| f >= INFLATED_VARIANCE_FRACTION)
            && self
                .outlier_count
                .is_some_and(|k| (k as f64) < MIN_MODE_FRACTION * self.runs as f64)
    }

    pub fn is_empty(&self) -> bool {
        Diagnostics { runs: 0, ..*self } == Diagnostics::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trend {
    /// Change from the first to the last run, relative to the median
    pub rel_change: f64,
    /// Two-sided p-value of the Mann–Kendall test
    pub p_value: f64,
}

/// Robust trend of `xs` over the run index: Theil–Sen slope (median of all
/// pairwise slopes) and the Mann–Kendall test with tie correction.
pub fn trend(xs: &[f64]) -> Option<Trend> {
    let n = xs.len();
    if n < MIN_TREND_SAMPLES || xs.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let median = crate::stats::basic::median(xs);
    if median <= 0.0 {
        return None;
    }

    // (run index, value), thinned evenly for long series
    let points: Vec<(f64, f64)> = if n > MAX_TREND_SAMPLES {
        (0..MAX_TREND_SAMPLES)
            .map(|i| i * (n - 1) / (MAX_TREND_SAMPLES - 1))
            .map(|i| (i as f64, xs[i]))
            .collect()
    } else {
        xs.iter().enumerate().map(|(i, &x)| (i as f64, x)).collect()
    };
    let m = points.len();

    let mut slopes = Vec::with_capacity(m * (m - 1) / 2);
    let mut s: i64 = 0;
    for (i, &(ti, xi)) in points.iter().enumerate() {
        for &(tj, xj) in &points[i + 1..] {
            slopes.push((xj - xi) / (tj - ti));
            s += match xj.total_cmp(&xi) {
                std::cmp::Ordering::Greater => 1,
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
            };
        }
    }
    slopes.sort_by(f64::total_cmp);
    let slope = crate::stats::basic::median(&slopes);
    let rel_change = slope * (n - 1) as f64 / median;

    // Variance of S with tie correction
    let mut sorted: Vec<f64> = points.iter().map(|&(_, x)| x).collect();
    sorted.sort_by(f64::total_cmp);
    let ties: f64 = sorted
        .chunk_by(|a, b| a == b)
        .map(|group| group.len() as f64)
        .filter(|&t| t > 1.0)
        .map(|t| t * (t - 1.0) * (2.0 * t + 5.0))
        .sum();
    let mf = m as f64;
    let var_s = (mf * (mf - 1.0) * (2.0 * mf + 5.0) - ties) / 18.0;

    let p_value = if var_s <= 0.0 {
        1.0
    } else {
        // Continuity correction
        let z = (s.abs() as f64 - 1.0).max(0.0) / var_s.sqrt();
        erfc(z / std::f64::consts::SQRT_2)
    };

    Some(Trend {
        rel_change,
        p_value,
    })
}

/// Sarle's bimodality coefficient, with sample skewness and excess kurtosis
/// corrected for bias. Values above 5/9 suggest bi- or multimodality.
pub fn bimodality_coefficient(xs: &[f64]) -> Option<f64> {
    let n = xs.len();
    if n < MIN_BIMODALITY_SAMPLES {
        return None;
    }
    let nf = n as f64;
    let mean = xs.iter().sum::<f64>() / nf;
    let moment = |k: i32| xs.iter().map(|x| (x - mean).powi(k)).sum::<f64>() / nf;
    let m2 = moment(2);
    if m2 <= f64::EPSILON * mean.abs().max(1.0) * mean.abs().max(1.0) {
        return None;
    }
    let g1 = moment(3) / m2.powf(1.5);
    let g2 = moment(4) / (m2 * m2) - 3.0;
    let skewness = g1 * (nf * (nf - 1.0)).sqrt() / (nf - 2.0);
    let kurtosis = ((nf + 1.0) * g2 + 6.0) * (nf - 1.0) / ((nf - 2.0) * (nf - 3.0));
    let bc = (skewness * skewness + 1.0)
        / (kurtosis + 3.0 * (nf - 1.0).powi(2) / ((nf - 2.0) * (nf - 3.0)));
    bc.is_finite().then_some(bc)
}

/// Whether the Gaussian kernel density estimate of `xs` has two separated
/// modes. The normal-reference bandwidth oversmooths, so a skewed unimodal
/// sample (the typical shape of run times) does not show spurious modes.
fn has_separated_modes(xs: &[f64]) -> bool {
    let n = xs.len();
    if n < MIN_BIMODALITY_SAMPLES {
        return false;
    }
    let mut sorted = xs.to_vec();
    sorted.sort_by(f64::total_cmp);
    let nf = n as f64;
    let mean = sorted.iter().sum::<f64>() / nf;
    let sd = (sorted.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nf - 1.0)).sqrt();
    // Normal-reference bandwidth: it oversmooths multimodal and skewed data,
    // so sampling noise in the tail of a skewed sample doesn't create modes.
    let h = 1.06 * sd * nf.powf(-0.2);
    if !(h > 0.0 && h.is_finite()) {
        return false;
    }

    let (lo, hi) = (sorted[0] - 3.0 * h, sorted[n - 1] + 3.0 * h);
    let step = (hi - lo) / (KDE_GRID_POINTS - 1) as f64;
    let grid: Vec<f64> = (0..KDE_GRID_POINTS).map(|i| lo + step * i as f64).collect();
    let density: Vec<f64> = grid
        .iter()
        .map(|g| {
            sorted
                .iter()
                .map(|x| (-0.5 * ((g - x) / h).powi(2)).exp())
                .sum::<f64>()
        })
        .collect();

    let mut peaks: Vec<usize> = (1..KDE_GRID_POINTS - 1)
        .filter(|&i| density[i] > density[i - 1] && density[i] >= density[i + 1])
        .collect();
    let valley_between = |a: usize, b: usize| {
        (a..=b)
            .min_by(|&x, &y| density[x].total_cmp(&density[y]))
            .unwrap_or(a)
    };

    // Merge neighbouring peaks that are not separated by a deep valley
    // (keeping the higher one), until all remaining valleys are deep.
    while let Some(k) = (0..peaks.len().saturating_sub(1)).find(|&k| {
        let valley = valley_between(peaks[k], peaks[k + 1]);
        density[valley] > MAX_VALLEY_RATIO * density[peaks[k]].min(density[peaks[k + 1]])
    }) {
        let lower = if density[peaks[k]] < density[peaks[k + 1]] {
            k
        } else {
            k + 1
        };
        peaks.remove(lower);
    }

    // Each mode owns the runs between the valleys around it; count the modes
    // with at least 10% of the runs.
    let mut boundaries = vec![f64::NEG_INFINITY];
    boundaries.extend(
        peaks
            .windows(2)
            .map(|pair| grid[valley_between(pair[0], pair[1])]),
    );
    boundaries.push(f64::INFINITY);
    let runs_below = |x: f64| sorted.partition_point(|&v| v < x);
    let significant_modes = boundaries
        .windows(2)
        .filter(|range| {
            let count = runs_below(range[1]) - runs_below(range[0]);
            count as f64 >= MIN_MODE_FRACTION * nf
        })
        .count();
    significant_modes >= 2
}

/// Fraction of the sample variance of `all` that disappears in `kept`.
fn removed_variance_fraction(all: &[f64], kept: &[f64]) -> Option<f64> {
    fn variance(v: &[f64]) -> f64 {
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (v.len() - 1) as f64
    }
    if kept.len() < 2 {
        return None;
    }
    let total = variance(all);
    if total <= 0.0 {
        return None;
    }
    Some((1.0 - variance(kept) / total).clamp(0.0, 1.0))
}

/// Complementary error function (Numerical Recipes `erfcc`, relative error
/// below 1.2e-7 everywhere).
fn erfc(x: f64) -> f64 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.5 * z);
    let r = t
        * (-z * z - 1.265_512_23
            + t * (1.000_023_68
                + t * (0.374_091_96
                    + t * (0.096_784_18
                        + t * (-0.186_288_06
                            + t * (0.278_868_07
                                + t * (-1.135_203_98
                                    + t * (1.488_515_87
                                        + t * (-0.822_152_23 + t * 0.170_872_77)))))))))
            .exp();
    if x >= 0.0 {
        r
    } else {
        2.0 - r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outlier_detection::OUTLIER_THRESHOLD;
    use rand::{rngs::StdRng, Rng, SeedableRng};

    /// Approximately normal noise (sum of uniforms), deterministic
    fn noise(n: usize, mean: f64, sd: f64, seed: u64) -> Vec<f64> {
        let mut rng = StdRng::seed_from_u64(seed);
        (0..n)
            .map(|_| {
                let s: f64 = (0..12).map(|_| rng.gen::<f64>()).sum::<f64>() - 6.0;
                mean + sd * s
            })
            .collect()
    }

    #[test]
    fn erfc_matches_reference_values() {
        assert!((erfc(0.0) - 1.0).abs() < 1e-7);
        assert!((erfc(1.0) - 0.157_299_207).abs() < 1e-7);
        assert!((erfc(-1.0) - 1.842_700_793).abs() < 1e-7);
        // Two-sided p for z = 2.5758 is 0.01
        assert!((erfc(2.5758 / std::f64::consts::SQRT_2) - 0.01).abs() < 1e-5);
    }

    #[test]
    fn linear_ramp_is_a_trend() {
        let xs: Vec<f64> = (0..30).map(|i| 0.1 + 0.001 * i as f64).collect();
        let t = trend(&xs).unwrap();
        // 0.029 increase over a median of ~0.1145
        assert!((t.rel_change - 0.029 / 0.1145).abs() < 0.01, "{t:?}");
        assert!(t.p_value < 1e-6);
        assert!(Diagnostics::compute(&xs, OUTLIER_THRESHOLD).has_trend());
    }

    #[test]
    fn decreasing_series_has_a_negative_trend() {
        let xs: Vec<f64> = (0..30).map(|i| 0.2 - 0.002 * i as f64).collect();
        let t = trend(&xs).unwrap();
        assert!(t.rel_change < -0.3 && t.p_value < 1e-6, "{t:?}");
    }

    #[test]
    fn white_noise_is_not_a_trend() {
        for seed in 0..20 {
            let xs = noise(50, 0.1, 0.005, seed);
            let d = Diagnostics::compute(&xs, OUTLIER_THRESHOLD);
            assert!(!d.has_trend(), "seed {seed}: {d:?}");
        }
    }

    #[test]
    fn constant_times_have_no_trend_and_no_bimodality() {
        let xs = vec![0.1; 40];
        let t = trend(&xs).unwrap();
        assert_eq!(t.rel_change, 0.0);
        assert_eq!(t.p_value, 1.0);
        let d = Diagnostics::compute(&xs, OUTLIER_THRESHOLD);
        assert!(!d.has_trend() && !d.is_multimodal() && !d.has_inflated_variance());
    }

    #[test]
    fn a_slow_first_run_alone_is_not_a_trend() {
        let mut xs = noise(30, 0.1, 0.003, 7);
        xs[0] = 0.5;
        assert!(!Diagnostics::compute(&xs, OUTLIER_THRESHOLD).has_trend());
    }

    #[test]
    fn long_series_are_thinned() {
        let xs: Vec<f64> = (0..20_000).map(|i| 1.0 + 1e-5 * i as f64).collect();
        let t = trend(&xs).unwrap();
        assert!((t.rel_change - 0.19999 / 1.099995).abs() < 1e-3, "{t:?}");
    }

    #[test]
    fn two_clusters_are_multimodal() {
        let mut xs = noise(30, 0.1, 0.002, 1);
        xs.extend(noise(30, 0.2, 0.002, 2));
        let d = Diagnostics::compute(&xs, OUTLIER_THRESHOLD);
        assert!(d.bimodality.unwrap() > BIMODALITY_THRESHOLD, "{d:?}");
        assert!(d.is_multimodal(), "{d:?}");
    }

    #[test]
    fn normal_and_skewed_samples_are_not_multimodal() {
        for seed in 0..20 {
            let xs = noise(100, 0.1, 0.005, seed);
            let d = Diagnostics::compute(&xs, OUTLIER_THRESHOLD);
            assert!(!d.is_multimodal(), "normal, seed {seed}: {d:?}");

            // Right-skewed, unimodal (typical for run times): exponential tail
            let mut rng = StdRng::seed_from_u64(seed);
            let xs: Vec<f64> = (0..100)
                .map(|_| 0.1 - 0.005 * (1.0 - rng.gen::<f64>()).ln())
                .collect();
            let d = Diagnostics::compute(&xs, OUTLIER_THRESHOLD);
            assert!(!d.is_multimodal(), "skewed, seed {seed}: {d:?}");
        }
    }

    #[test]
    fn unequal_clusters_are_multimodal() {
        let mut xs = noise(64, 0.05, 0.001, 4);
        xs.extend(noise(16, 0.08, 0.001, 5));
        let d = Diagnostics::compute(&xs, OUTLIER_THRESHOLD);
        assert!(d.is_multimodal(), "{d:?}");
        // The minority cluster is not "a few outliers"
        assert!(!d.has_inflated_variance(), "{d:?}");
    }

    #[test]
    fn lognormal_samples_are_not_multimodal() {
        for seed in 0..20 {
            let xs: Vec<f64> = noise(200, 0.0, 0.5, seed)
                .into_iter()
                .map(|z| 0.1 * z.exp())
                .collect();
            let d = Diagnostics::compute(&xs, OUTLIER_THRESHOLD);
            assert!(!d.is_multimodal(), "seed {seed}: {d:?}");
        }
    }

    #[test]
    fn a_few_huge_outliers_inflate_the_variance() {
        let mut xs = noise(40, 0.1, 0.002, 3);
        xs[10] = 0.5;
        xs[20] = 0.6;
        xs[30] = 0.55;
        let d = Diagnostics::compute(&xs, OUTLIER_THRESHOLD);
        assert!(d.outlier_variance_fraction.unwrap() > 0.9, "{d:?}");
        assert_eq!(d.outlier_count, Some(3));
        assert!(d.has_inflated_variance());
        // The outliers must not make the sample look multimodal
        assert!(!d.is_multimodal(), "{d:?}");
    }

    #[test]
    fn short_samples_give_no_diagnostics() {
        let d = Diagnostics::compute(&[0.1, 0.2, 0.3], OUTLIER_THRESHOLD);
        assert!(d.is_empty());
    }
}
