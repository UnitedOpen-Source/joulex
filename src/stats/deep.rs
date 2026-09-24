use criterion_stats::univariate::Sample;

#[derive(Debug, Clone, PartialEq)]
pub struct DeepStats {
    pub mean_ci_lower: f64,
    pub mean_ci_upper: f64,
    pub median_ci_lower: f64,
    pub median_ci_upper: f64,
    pub std_dev_ci_lower: f64,
    pub std_dev_ci_upper: f64,
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
