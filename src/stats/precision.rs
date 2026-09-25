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
/// mean (0.01 = ±1%), accounting for the uncertainty of a subtracted baseline
/// (`--subtract`) if present. `None` for fewer than 2 values or a mean ≤ 0.
pub fn relative_ci_half_width_with_baseline(
    xs: &[f64],
    baseline_stddev: Option<f64>,
    baseline_runs: usize,
) -> Option<f64> {
    let n = xs.len();
    if n < 2 {
        return None;
    }
    let mean = xs.iter().sum::<f64>() / n as f64;
    if mean.is_nan() || mean <= 0.0 {
        return None;
    }
    let is_constant = xs.iter().all(|&x| x == xs[0]);
    let se_sq_cmd = if is_constant {
        0.0
    } else {
        let variance = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
        variance / n as f64
    };

    let (se_sq_base, n_base) = match (baseline_stddev, baseline_runs) {
        (Some(sd), runs) if runs > 0 && sd > 0.0 => (sd.powi(2) / runs as f64, runs),
        _ => (0.0, 0),
    };

    if se_sq_cmd == 0.0 && se_sq_base == 0.0 {
        return Some(0.0);
    }

    let combined_se = (se_sq_cmd + se_sq_base).sqrt();

    let df = if se_sq_base == 0.0 {
        n - 1
    } else if se_sq_cmd == 0.0 {
        n_base.saturating_sub(1).max(1)
    } else {
        let num = (se_sq_cmd + se_sq_base).powi(2);
        let den = (se_sq_cmd.powi(2) / (n - 1) as f64)
            + (se_sq_base.powi(2) / (n_base - 1).max(1) as f64);
        if den > 0.0 {
            (num / den).round() as usize
        } else {
            n - 1
        }
        .max(1)
    };

    Some(t_975(df) * combined_se / mean)
}

/// Half-width of the 95% confidence interval of the mean, relative to the
/// mean (0.01 = ±1%). `None` for fewer than 2 values or a mean ≤ 0.
pub fn relative_ci_half_width(xs: &[f64]) -> Option<f64> {
    relative_ci_half_width_with_baseline(xs, None, 0)
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

    #[test]
    fn half_width_with_baseline() {
        let xs = [9.0, 11.0, 9.0, 11.0];
        let without_base = relative_ci_half_width(&xs).unwrap();
        // With a noisy baseline (sd 1.0, 4 runs), the confidence interval is wider
        let with_base = relative_ci_half_width_with_baseline(&xs, Some(1.0), 4).unwrap();
        assert!(with_base > without_base);

        // Constant runs with zero baseline variance is exactly 0
        assert_eq!(
            relative_ci_half_width_with_baseline(&[2.0, 2.0], Some(0.0), 5),
            Some(0.0)
        );

        // Constant runs with noisy baseline has non-zero uncertainty
        let const_with_noise =
            relative_ci_half_width_with_baseline(&[10.0, 10.0, 10.0], Some(1.0), 4).unwrap();
        assert!(const_with_noise > 0.0);
    }
}
