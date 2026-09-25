//! `--target-precision`: how precisely the mean run time is known.

/// Two-sided 95% (i.e. the 97.5% quantile) of Student's t distribution with
/// `df` degrees of freedom.
pub fn t_975(df: usize) -> f64 {
    const T: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];
    match df {
        0 => f64::INFINITY,
        1..=30 => T[df - 1],
        31..=40 => 2.021,
        41..=60 => 2.000,
        61..=120 => 1.980,
        _ => 1.960,
    }
}

/// Half-width of the 95% confidence interval of the mean, relative to the
/// mean (0.01 = ±1%). `None` for fewer than 2 values or a mean ≤ 0.
pub fn relative_ci_half_width(xs: &[f64]) -> Option<f64> {
    let n = xs.len();
    if n < 2 {
        return None;
    }
    let mean = xs.iter().sum::<f64>() / n as f64;
    if mean.is_nan() || mean <= 0.0 {
        return None;
    }
    // Exactly 0 for constant samples (the rounding of the mean would
    // otherwise leave a variance of ~1e-33)
    if xs.iter().all(|&x| x == xs[0]) {
        return Some(0.0);
    }
    let variance = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
    let standard_error = (variance / n as f64).sqrt();
    Some(t_975(n - 1) * standard_error / mean)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_quantiles() {
        assert_eq!(t_975(0), f64::INFINITY);
        assert_eq!(t_975(1), 12.706);
        assert_eq!(t_975(30), 2.042);
        assert_eq!(t_975(1000), 1.960);
    }

    #[test]
    fn half_width() {
        assert_eq!(relative_ci_half_width(&[1.0]), None);
        assert_eq!(relative_ci_half_width(&[0.0, 0.0]), None);
        assert_eq!(relative_ci_half_width(&[2.0, 2.0, 2.0]), Some(0.0));
        // Ten times 0.1: the mean is 0.10000000000000002, still exactly 0
        assert_eq!(relative_ci_half_width(&[0.1; 10]), Some(0.0));
        // mean 10, sd 1, n = 4: 3.182 * 0.5 / 10
        let xs = [9.0, 11.0, 9.0, 11.0];
        let sd = (4.0f64 / 3.0).sqrt();
        let expected = 3.182 * sd / 2.0 / 10.0;
        assert!((relative_ci_half_width(&xs).unwrap() - expected).abs() < 1e-12);
        // More runs of the same noise: narrower
        let many: Vec<f64> = (0..100)
            .map(|i| if i % 2 == 0 { 9.0 } else { 11.0 })
            .collect();
        assert!(relative_ci_half_width(&many).unwrap() < 0.021);
    }
}
