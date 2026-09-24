//! Simple descriptive statistics that complement mean ± σ.

/// Percentile `p` (0–100) of an **ascending sorted** sample, using linear
/// interpolation between the closest ranks (numpy's default method).
pub fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() || !(0.0..=100.0).contains(&p) {
        return None;
    }
    let rank = p / 100.0 * (sorted.len() - 1) as f64;
    let (lo, hi) = (rank.floor() as usize, rank.ceil() as usize);
    Some(sorted[lo] + (sorted[hi] - sorted[lo]) * (rank - lo as f64))
}

/// Geometric mean; None if the sample is empty or contains a value ≤ 0.
pub fn geometric_mean(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() || xs.iter().any(|&x| x <= 0.0 || !x.is_finite()) {
        return None;
    }
    Some((xs.iter().map(|x| x.ln()).sum::<f64>() / xs.len() as f64).exp())
}

/// p05, p25, p75, p95 of an unsorted sample.
pub fn quartiles_and_tails(xs: &[f64]) -> Option<[f64; 4]> {
    let mut sorted = xs.to_vec();
    sorted.sort_by(f64::total_cmp);
    Some([
        percentile(&sorted, 5.0)?,
        percentile(&sorted, 25.0)?,
        percentile(&sorted, 75.0)?,
        percentile(&sorted, 95.0)?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn percentile_interpolates_linearly() {
        let xs = [1.0, 2.0, 3.0, 4.0];
        assert_relative_eq!(percentile(&xs, 50.0).unwrap(), 2.5);
        assert_relative_eq!(percentile(&xs, 0.0).unwrap(), 1.0);
        assert_relative_eq!(percentile(&xs, 100.0).unwrap(), 4.0);
        assert_relative_eq!(percentile(&xs, 25.0).unwrap(), 1.75);
        assert_eq!(percentile(&[], 50.0), None);
        assert_eq!(percentile(&xs, 101.0), None);
        assert_relative_eq!(percentile(&[7.0], 95.0).unwrap(), 7.0);
    }

    #[test]
    fn geometric_mean_of_positive_values() {
        assert_relative_eq!(geometric_mean(&[1.0, 4.0]).unwrap(), 2.0);
        assert_relative_eq!(geometric_mean(&[2.0, 2.0, 2.0]).unwrap(), 2.0);
        assert_eq!(geometric_mean(&[1.0, 0.0]), None);
        assert_eq!(geometric_mean(&[]), None);
    }

    #[test]
    fn quartiles_and_tails_sorts_its_input() {
        let [p05, p25, p75, p95] = quartiles_and_tails(&[4.0, 1.0, 3.0, 2.0, 5.0]).unwrap();
        assert_relative_eq!(p05, 1.2);
        assert_relative_eq!(p25, 2.0);
        assert_relative_eq!(p75, 4.0);
        assert_relative_eq!(p95, 4.8);
    }
}
