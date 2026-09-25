use crate::util::units::{auto_decimals, precision, Precision, Second, Unit};

/// Format the given duration as a string. The output-unit can be enforced by setting `unit` to
/// `Some(target_unit)`. If `unit` is `None`, it will be determined automatically.
pub fn format_duration(duration: Second, unit: Option<Unit>) -> String {
    let (duration_fmt, _) = format_duration_unit(duration, unit);
    duration_fmt
}

/// Like `format_duration`, but returns the target unit as well.
pub fn format_duration_unit(duration: Second, unit: Option<Unit>) -> (String, Unit) {
    let (out_str, out_unit) = format_duration_value(duration, unit);

    (format!("{} {}", out_str, out_unit.short_name()), out_unit)
}

/// Like `format_duration`, but returns the target unit as well.
pub fn format_duration_value(duration: Second, unit: Option<Unit>) -> (String, Unit) {
    if (duration < 0.001 && unit.is_none()) || unit == Some(Unit::MicroSecond) {
        (Unit::MicroSecond.format(duration), Unit::MicroSecond)
    } else if (duration < 1.0 && unit.is_none()) || unit == Some(Unit::MilliSecond) {
        (Unit::MilliSecond.format(duration), Unit::MilliSecond)
    } else {
        (Unit::Second.format(duration), Unit::Second)
    }
}

/// Mean and σ as values in `unit` (without the unit name). With
/// `--precision auto`, both use the decimals that keep two significant digits
/// of σ, so they are always consistent: "120.2 ± 1.5", "12.34 ± 0.41".
pub fn format_mean_stddev_values(
    mean: Second,
    stddev: Option<Second>,
    unit: Unit,
) -> (String, Option<String>) {
    format_mean_stddev_with(precision(), mean, stddev, unit)
}

fn format_mean_stddev_with(
    precision: Precision,
    mean: Second,
    stddev: Option<Second>,
    unit: Unit,
) -> (String, Option<String>) {
    let decimals = match (precision, stddev) {
        (Precision::Auto, Some(stddev)) => auto_decimals(stddev * unit.per_second()),
        _ => None,
    };
    match decimals {
        Some(decimals) => (
            unit.format_decimals(mean, decimals),
            stddev.map(|s| unit.format_decimals(s, decimals)),
        ),
        None => (unit.format(mean), stddev.map(|s| unit.format(s))),
    }
}

#[test]
fn test_auto_precision_uses_the_decimals_of_stddev() {
    let auto =
        |mean, stddev, unit| format_mean_stddev_with(Precision::Auto, mean, Some(stddev), unit);
    assert_eq!(
        auto(120.16350913, 1.488, Unit::Second),
        ("120.2".into(), Some("1.5".into()))
    );
    assert_eq!(
        auto(0.012341, 0.00041, Unit::MilliSecond),
        ("12.34".into(), Some("0.41".into()))
    );
    assert_eq!(
        auto(0.0000184, 0.0000007, Unit::MicroSecond),
        ("18.40".into(), Some("0.70".into()))
    );
    assert_eq!(
        auto(250.0, 23.4, Unit::Second),
        ("250".into(), Some("23".into()))
    );
    // Without σ (or σ = 0): the default decimals
    assert_eq!(
        format_mean_stddev_with(Precision::Auto, 1.3, None, Unit::Second),
        ("1.300".into(), None)
    );
    assert_eq!(
        auto(1.3, 0.0, Unit::Second),
        ("1.300".into(), Some("0.000".into()))
    );
}

#[test]
fn test_auto_decimals() {
    use crate::util::units::auto_decimals;
    assert_eq!(auto_decimals(1.488), Some(1));
    assert_eq!(auto_decimals(0.041), Some(3));
    assert_eq!(auto_decimals(23.4), Some(0));
    assert_eq!(auto_decimals(1e-12), Some(9)); // clamped
    assert_eq!(auto_decimals(0.0), None);
    assert_eq!(auto_decimals(f64::NAN), None);
}

#[test]
fn test_format_decimals() {
    assert_eq!(Unit::Second.format_decimals(1.23456, 2), "1.23");
    assert_eq!(Unit::MilliSecond.format_decimals(0.0123456, 3), "12.346");
    assert_eq!(Unit::MicroSecond.format_decimals(0.0000123, 0), "12");
}

#[test]
fn test_format_duration_unit_basic() {
    let (out_str, out_unit) = format_duration_unit(1.3, None);

    assert_eq!("1.300 s", out_str);
    assert_eq!(Unit::Second, out_unit);

    let (out_str, out_unit) = format_duration_unit(1.0, None);

    assert_eq!("1.000 s", out_str);
    assert_eq!(Unit::Second, out_unit);

    let (out_str, out_unit) = format_duration_unit(0.999, None);

    assert_eq!("999.0 ms", out_str);
    assert_eq!(Unit::MilliSecond, out_unit);

    let (out_str, out_unit) = format_duration_unit(0.0005, None);

    assert_eq!("500.0 µs", out_str);
    assert_eq!(Unit::MicroSecond, out_unit);

    let (out_str, out_unit) = format_duration_unit(0.0, None);

    assert_eq!("0.0 µs", out_str);
    assert_eq!(Unit::MicroSecond, out_unit);

    let (out_str, out_unit) = format_duration_unit(1000.0, None);

    assert_eq!("1000.000 s", out_str);
    assert_eq!(Unit::Second, out_unit);
}

#[test]
fn test_format_duration_unit_with_unit() {
    let (out_str, out_unit) = format_duration_unit(1.3, Some(Unit::Second));

    assert_eq!("1.300 s", out_str);
    assert_eq!(Unit::Second, out_unit);

    let (out_str, out_unit) = format_duration_unit(1.3, Some(Unit::MilliSecond));

    assert_eq!("1300.0 ms", out_str);
    assert_eq!(Unit::MilliSecond, out_unit);

    let (out_str, out_unit) = format_duration_unit(1.3, Some(Unit::MicroSecond));

    assert_eq!("1300000.0 µs", out_str);
    assert_eq!(Unit::MicroSecond, out_unit);
}
