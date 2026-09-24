use criterion_stats::univariate::Sample;
use criterion_stats::Tails;

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

/// Compute 95% bootstrap confidence intervals for mean, median, and std_dev
pub fn compute_deep_stats(data: &[f64]) -> Option<DeepStats> {
    if data.len() < 3 {
        return None;
    }

    let sample = Sample::new(data);
    let nresamples = 5000;
    let cl = 0.95;

    // Bootstrap distributions for (mean, median, std_dev)
    let (mean_dist, median_dist, stddev_dist) = sample.bootstrap(nresamples, |s| {
        (s.mean(), s.percentiles().median(), s.std_dev(None))
    });

    let (mean_ci_lower, mean_ci_upper) = mean_dist.confidence_interval(cl);
    let (median_ci_lower, median_ci_upper) = median_dist.confidence_interval(cl);
    let (std_dev_ci_lower, std_dev_ci_upper) = stddev_dist.confidence_interval(cl);

    Some(DeepStats {
        mean_ci_lower,
        mean_ci_upper,
        median_ci_lower,
        median_ci_upper,
        std_dev_ci_lower,
        std_dev_ci_upper,
    })
}

/// Perform a two-sample bootstrap hypothesis test comparing sample `a` and sample `b`
pub fn compare_samples(a: &[f64], b: &[f64]) -> Option<ComparisonStats> {
    if a.len() < 3 || b.len() < 3 {
        return None;
    }

    let sample_a = Sample::new(a);
    let sample_b = Sample::new(b);

    let t_stat = sample_a.t(sample_b);
    if !t_stat.is_finite() {
        // Zero variance in both samples:
        // If means are equal, the samples are identical -> p = 1.0, no difference
        let same = (sample_a.mean() - sample_b.mean()).abs()
            <= f64::EPSILON * sample_a.mean().abs().max(1.0);
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

    let nresamples = 5000;
    let (dist,) =
        criterion_stats::univariate::mixed::bootstrap(sample_a, sample_b, nresamples, |s1, s2| {
            (s1.t(s2),)
        });
    let p_val = dist.p_value(t_stat, &Tails::Two);

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
}
