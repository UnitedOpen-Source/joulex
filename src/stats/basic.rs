//! Mean, median and sample standard deviation.
//!
//! Drop-in replacements for the functions previously taken from the
//! unmaintained `statistical` crate (#29), with the same signatures and
//! semantics, so call sites are unchanged.

/// Arithmetic mean. Like `statistical::mean`, an empty slice gives NaN.
pub fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// Median; the mean of the two middle values for an even number of values.
///
/// # Panics
/// If `values` is empty (as `statistical::median`).
pub fn median(values: &[f64]) -> f64 {
    assert!(
        !values.is_empty(),
        "median requires at least one data point"
    );
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

/// Sample standard deviation (with Bessel's correction, i.e. dividing by
/// `n - 1`). `center` is the mean if already known; otherwise it is computed.
///
/// # Panics
/// If `values` has fewer than two elements (as `statistical::standard_deviation`).
pub fn standard_deviation(values: &[f64], center: Option<f64>) -> f64 {
    assert!(
        values.len() > 1,
        "standard deviation requires at least two data points"
    );
    let center = center.unwrap_or_else(|| mean(values));
    let sum_of_squares: f64 = values.iter().map(|x| (x - center).powi(2)).sum();
    (sum_of_squares / (values.len() - 1) as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_mean() {
        assert_relative_eq!(mean(&[1.0, 2.0, 3.0, 4.0]), 2.5);
        assert_relative_eq!(mean(&[5.0]), 5.0);
        assert!(mean(&[]).is_nan());
    }

    #[test]
    fn test_median() {
        assert_relative_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_relative_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
        assert_relative_eq!(median(&[7.0]), 7.0);
    }

    #[test]
    #[should_panic(expected = "at least one data point")]
    fn test_median_of_empty_slice_panics() {
        median(&[]);
    }

    #[test]
    fn test_standard_deviation() {
        // Sample standard deviation of 2, 4, 4, 4, 5, 5, 7, 9 is sqrt(32 / 7)
        let xs = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        assert_relative_eq!(standard_deviation(&xs, None), (32.0f64 / 7.0).sqrt());
        assert_relative_eq!(standard_deviation(&xs, Some(5.0)), (32.0f64 / 7.0).sqrt());
        assert_relative_eq!(standard_deviation(&[1.0, 1.0], None), 0.0);
    }

    #[test]
    #[should_panic(expected = "at least two data points")]
    fn test_standard_deviation_of_single_value_panics() {
        standard_deviation(&[1.0], None);
    }
}
