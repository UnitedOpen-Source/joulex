//! Tests for the terminal report format.

mod common;
use common::hyperfine;

use predicates::prelude::*;

#[test]
fn range_line_shows_the_median() {
    hyperfine()
        .arg("--debug-mode")
        .arg("--runs=3")
        .arg("--style=basic")
        .arg("sleep 0.1")
        .assert()
        .success()
        .stdout(predicate::str::contains("Range (min … median … max)"))
        .stdout(predicate::str::contains("100.0 ms … 100.0 ms … 100.0 ms"));
}

#[test]
fn deep_stats_show_percentiles_and_geometric_mean() {
    hyperfine()
        .arg("--debug-mode")
        .arg("--runs=5")
        .arg("--style=basic")
        .arg("--deep-stats")
        .arg("sleep 0.1")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Percentiles:        [p05: 100.0 ms, p25: 100.0 ms, p75: 100.0 ms, p95: 100.0 ms (IQR 0.0 ms), geometric mean: 100.0 ms]",
        ));
}

#[test]
fn percentiles_are_not_shown_without_deep_stats() {
    hyperfine()
        .arg("--debug-mode")
        .arg("--runs=5")
        .arg("--style=basic")
        .arg("sleep 0.1")
        .assert()
        .success()
        .stdout(predicate::str::contains("Percentiles").not());
}
