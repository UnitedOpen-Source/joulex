//! Tests for --output-metric (#90).

mod common;
use common::hyperfine;

use predicates::prelude::*;

#[test]
fn conflicts_with_show_output_and_output_and_until() {
    hyperfine()
        .args(["--output-metric", "lat=([0-9]+)", "--show-output", "echo 1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));

    hyperfine()
        .args([
            "--output-metric",
            "lat=([0-9]+)",
            "--output",
            "pipe",
            "echo 1",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));

    hyperfine()
        .args([
            "--output-metric",
            "lat=([0-9]+)",
            "--until",
            "READY",
            "echo 1",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn rejects_invalid_syntax_missing_equals() {
    hyperfine()
        .args(["--output-metric", "latency_no_equals", "echo 1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected 'NAME=REGEX'"));
}

#[test]
fn rejects_invalid_name() {
    hyperfine()
        .args(["--output-metric", "my-metric=([0-9]+)", "echo 1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "names must contain only ASCII alphanumeric characters and underscores",
        ));
}

#[test]
fn rejects_missing_capture_group() {
    hyperfine()
        .args(["--output-metric", "lat=[0-9]+", "echo 1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "must contain at least one capture group",
        ));
}

#[test]
fn rejects_duplicate_metric_name() {
    hyperfine()
        .args([
            "--output-metric",
            "lat=([0-9]+)",
            "--output-metric",
            "lat=([a-z]+)",
            "echo 1",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate metric name 'lat'"));
}

#[test]
fn single_metric_extracted_and_displayed() {
    hyperfine()
        .args([
            "--runs",
            "2",
            "--output-metric",
            "latency=time=([0-9.]+)ms",
            "echo 'time=42.5ms'",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("latency (mean ± σ):"))
        .stdout(predicate::str::contains("42.500"));
}

#[test]
fn multiple_metrics_extracted_and_displayed() {
    hyperfine()
        .args([
            "--runs",
            "2",
            "--output-metric",
            "latency=time=([0-9.]+)ms",
            "--output-metric",
            "queries=queries=([0-9]+)",
            "echo 'time=10.0ms queries=500'",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("latency (mean ± σ):"))
        .stdout(predicate::str::contains("queries (mean ± σ):"))
        .stdout(predicate::str::contains("10.000"))
        .stdout(predicate::str::contains("500.000"));
}

#[test]
fn missing_metric_fails_command() {
    hyperfine()
        .args([
            "--runs",
            "2",
            "--output-metric",
            "latency=time=([0-9.]+)ms",
            "echo 'no timing here'",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Metric 'latency' could not be extracted from output in the first benchmark run",
        ));
}

#[test]
fn missing_metric_ignored_with_ignore_failure() {
    hyperfine()
        .args([
            "-i",
            "--runs",
            "2",
            "--output-metric",
            "latency=time=([0-9.]+)ms",
            "echo 'no timing here'",
        ])
        .assert()
        .success();
}

#[test]
fn export_json_contains_custom_metrics() {
    let dir = tempfile::tempdir().unwrap();
    let json_path = dir.path().join("results.json");

    hyperfine()
        .args([
            "--runs",
            "2",
            "--output-metric",
            "latency=time=([0-9.]+)ms",
            "--export-json",
            json_path.to_str().unwrap(),
            "echo 'time=15.2ms'",
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&json_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    let results = parsed.get("results").unwrap().as_array().unwrap();
    assert_eq!(results.len(), 1);

    let custom_metrics = results[0].get("custom_metrics").unwrap();
    let latency_values = custom_metrics.get("latency").unwrap().as_array().unwrap();
    assert_eq!(latency_values.len(), 2);
    assert_eq!(latency_values[0].as_f64().unwrap(), 15.2);
    assert_eq!(latency_values[1].as_f64().unwrap(), 15.2);

    let summary = results[0].get("custom_metrics_summary").unwrap();
    let latency_summary = summary.get("latency").unwrap();
    assert_eq!(latency_summary.get("mean").unwrap().as_f64().unwrap(), 15.2);
    assert_eq!(
        latency_summary.get("median").unwrap().as_f64().unwrap(),
        15.2
    );
    assert_eq!(latency_summary.get("min").unwrap().as_f64().unwrap(), 15.2);
    assert_eq!(latency_summary.get("max").unwrap().as_f64().unwrap(), 15.2);
}

#[test]
fn export_csv_contains_custom_metric_columns() {
    let dir = tempfile::tempdir().unwrap();
    let csv_path = dir.path().join("results.csv");

    hyperfine()
        .args([
            "--runs",
            "2",
            "--output-metric",
            "latency=time=([0-9.]+)ms",
            "--export-csv",
            csv_path.to_str().unwrap(),
            "echo 'time=25.0ms'",
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(&csv_path).unwrap();
    let mut lines = content.lines();
    let header = lines.next().unwrap();
    assert!(header.contains("mean_latency,stddev_latency,median_latency,min_latency,max_latency"));

    let data_row = lines.next().unwrap();
    assert!(data_row.contains("25,0,25,25,25"));
}
