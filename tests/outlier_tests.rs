//! Tests for --discard-outliers.

#![cfg(unix)]

mod common;
use common::hyperfine;

use predicates::prelude::*;

fn run_json(args: &[&str]) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    let export = dir.path().join("out.json");
    hyperfine()
        .args(args)
        .arg("--export-json")
        .arg(&export)
        .assert()
        .success();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&export).unwrap()).unwrap();
    json["results"][0].clone()
}

/// Sleeps 0.5 s in the given iterations, otherwise returns immediately.
fn slow_in(iterations: &str) -> String {
    format!("sh -c 'case $JOULEX_ITERATION in {iterations}) sleep 0.5;; esac; true'")
}

#[test]
fn discards_a_single_outlier() {
    let result = run_json(&["-N", "--runs=20", "--discard-outliers=100", &slow_in("7")]);

    assert_eq!(result["discarded_outliers"], serde_json::json!([7]));
    assert_eq!(result["times"].as_array().unwrap().len(), 19);
    assert_eq!(result["exit_codes"].as_array().unwrap().len(), 19);
    assert!(result["max"].as_f64().unwrap() < 0.2, "{result}");
}

#[test]
fn prints_the_number_of_discarded_outliers() {
    hyperfine()
        .args([
            "-N",
            "--runs=20",
            "--style=basic",
            "--discard-outliers=100",
            &slow_in("7"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("19 runs (1 outlier discarded)"));
}

#[test]
fn refuses_to_discard_more_than_five_percent() {
    let dir = tempfile::tempdir().unwrap();
    let export = dir.path().join("out.json");
    hyperfine()
        .args(["-N", "--runs=20", "--discard-outliers", &slow_in("3|7|11")])
        .arg("--export-json")
        .arg(&export)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "look like outliers, so none were discarded",
        ));
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&export).unwrap()).unwrap();
    assert_eq!(json["results"][0]["times"].as_array().unwrap().len(), 20);
    assert!(json["results"][0].get("discarded_outliers").is_none());
}

#[test]
fn reports_original_run_numbers_together_with_omitted_failed_runs() {
    // Run 2 fails and is omitted first; run 7 is slow. The discarded run must be
    // reported as 7 (its original number), not 6 (its position after omission).
    let command = "sh -c 'case $JOULEX_ITERATION in 2) exit 1;; 7) sleep 0.5;; esac; true'";
    let result = run_json(&[
        "-N",
        "--runs=20",
        "--ignore-failure",
        "--omit-failed-runs",
        "--discard-outliers=100",
        command,
    ]);

    assert_eq!(result["omitted_failed_runs"][0]["index"], 2);
    assert_eq!(result["discarded_outliers"], serde_json::json!([7]));
    assert_eq!(result["times"].as_array().unwrap().len(), 18);
}

#[test]
fn rejects_invalid_thresholds() {
    for value in ["-3", "0", "abc"] {
        hyperfine()
            .arg(format!("--discard-outliers={value}"))
            .arg("true")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Invalid threshold"));
    }
}
