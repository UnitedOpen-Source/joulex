/// Largest value. NaN values are ignored (they used to panic); an empty slice
/// gives NaN.
pub fn max(vals: &[f64]) -> f64 {
    vals.iter()
        .copied()
        .filter(|v| !v.is_nan())
        .max_by(f64::total_cmp)
        .unwrap_or(f64::NAN)
}

/// Smallest value. NaN values are ignored (they used to panic); an empty
/// slice gives NaN.
pub fn min(vals: &[f64]) -> f64 {
    vals.iter()
        .copied()
        .filter(|v| !v.is_nan())
        .min_by(f64::total_cmp)
        .unwrap_or(f64::NAN)
}

#[test]
fn test_min_max_do_not_panic() {
    assert!(max(&[]).is_nan());
    assert!(min(&[]).is_nan());
    assert_eq!(max(&[1.0, f64::NAN, 3.0]), 3.0);
    assert_eq!(min(&[f64::NAN, 2.0, 1.0]), 1.0);
    assert!(max(&[f64::NAN]).is_nan());
}

#[test]
fn test_max() {
    let assert_float_eq = |a: f64, b: f64| {
        assert!((a - b).abs() < f64::EPSILON);
    };

    assert_float_eq(1.0, max(&[1.0]));
    assert_float_eq(-1.0, max(&[-1.0]));
    assert_float_eq(-1.0, max(&[-2.0, -1.0]));
    assert_float_eq(1.0, max(&[-1.0, 1.0]));
    assert_float_eq(1.0, max(&[-1.0, 1.0, 0.0]));
}
