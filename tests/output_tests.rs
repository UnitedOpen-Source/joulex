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

#[test]
fn summary_shows_absolute_means_and_differences() {
    hyperfine()
        .arg("--debug-mode")
        .arg("--style=basic")
        .arg("sleep 2")
        .arg("sleep 1")
        .arg("sleep 0.5")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "sleep 0.5 ran\n    2.00 ± 0.00 times faster than sleep 1 (1.000 s, +0.500 s)\n    4.00 ± 0.00 times faster than sleep 2 (2.000 s, +1.500 s)",
        ));
}

#[test]
fn summary_differences_are_negative_for_commands_faster_than_the_reference() {
    hyperfine()
        .arg("--debug-mode")
        .arg("--style=basic")
        .arg("--reference=sleep 2")
        .arg("sleep 1")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "2.00 ± 0.00 times slower than sleep 1 (1.000 s, −1.000 s)",
        ));
}

#[test]
fn style_none_suppresses_progress_and_terminal_output() {
    hyperfine()
        .arg("--debug-mode")
        .arg("--runs=2")
        .arg("--style=none")
        .arg("sleep 0.1")
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("ETA").not());
}

#[test]
fn style_basic_and_color_suppress_interactive_progress_bar() {
    hyperfine()
        .arg("--debug-mode")
        .arg("--runs=2")
        .arg("--style=basic")
        .arg("sleep 0.1")
        .assert()
        .success()
        .stderr(predicate::str::contains("ETA").not())
        .stdout(predicate::str::contains("Benchmark 1: sleep 0.1"));

    hyperfine()
        .arg("--debug-mode")
        .arg("--runs=2")
        .arg("--style=color")
        .arg("sleep 0.1")
        .assert()
        .success()
        .stderr(predicate::str::contains("ETA").not())
        .stdout(predicate::str::contains("sleep 0.1"));
}

#[test]
fn style_full_executes_benchmarks_cleanly() {
    hyperfine()
        .arg("--debug-mode")
        .arg("--runs=2")
        .arg("--style=full")
        .arg("sleep 0.1")
        .assert()
        .success()
        .stdout(predicate::str::contains("sleep 0.1"));
}
