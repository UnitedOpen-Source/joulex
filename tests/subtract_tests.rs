//! Tests for --subtract (#56).

mod common;
use common::hyperfine;

use predicates::prelude::*;
use serde_json::Value;

fn results(args: &[&str]) -> Vec<Value> {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("out.json");
    hyperfine()
        .args(["--debug-mode", "--style=basic", "--export-json"])
        .arg(&json)
        .args(args)
        .assert()
        .success();
    let json: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    json["results"].as_array().unwrap().clone()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn the_baseline_mean_is_subtracted_from_every_run() {
    let results = results(&[
        "--runs=3",
        "--subtract",
        "sleep 0.05",
        "sleep 0.2",
        "sleep 0.3",
    ]);
    for (result, net) in results.iter().zip([0.15, 0.25]) {
        assert!(close(result["mean"].as_f64().unwrap(), net), "{result}");
        for t in result["times"].as_array().unwrap() {
            assert!(close(t.as_f64().unwrap(), net));
        }
        assert_eq!(result["baseline"]["command"], "sleep 0.05");
        assert!(close(result["baseline"]["mean"].as_f64().unwrap(), 0.05));
        assert_eq!(result["baseline"]["runs"], 3);
        assert!(result["baseline"].get("clamped_runs").is_none());
    }
}

#[test]
fn the_baseline_is_shown_and_marked() {
    hyperfine()
        .args([
            "--debug-mode",
            "--runs=3",
            "--style=basic",
            "--subtract",
            "sleep 0.05",
            "sleep 0.2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Baseline (subtracted): sleep 0.05",
        ))
        .stdout(predicate::str::contains("3 runs (baseline subtracted)"));
}

#[test]
fn a_larger_baseline_clamps_to_zero_and_warns() {
    let results = results(&["--runs=3", "--subtract", "sleep 0.5", "sleep 0.2"]);
    assert_eq!(results[0]["mean"], 0.0);
    assert_eq!(results[0]["baseline"]["clamped_runs"], 3);

    hyperfine()
        .args([
            "--debug-mode",
            "--runs=3",
            "--subtract",
            "sleep 0.5",
            "sleep 0.2",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "slower than the benchmarked command in 3 of 3 runs",
        ));
}

#[test]
fn works_with_round_robin() {
    let results = results(&[
        "--runs=3",
        "--schedule=round-robin",
        "--subtract",
        "sleep 0.05",
        "sleep 0.2",
        "sleep 0.3",
    ]);
    assert!(close(results[0]["mean"].as_f64().unwrap(), 0.15));
    assert!(close(results[1]["mean"].as_f64().unwrap(), 0.25));
}

#[test]
fn without_the_option_nothing_changes() {
    let results = results(&["--runs=2", "sleep 0.2"]);
    assert!(close(results[0]["mean"].as_f64().unwrap(), 0.2));
    assert!(results[0].get("baseline").is_none());
}

#[cfg(unix)]
#[test]
fn a_failing_baseline_is_an_error() {
    hyperfine()
        .args(["--runs=2", "--subtract", "exit 3", "true"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The '--subtract' baseline 'exit 3' failed",
        ));
}

#[test]
fn baseline_records_net_mean_stderr_in_json() {
    let results = results(&["--runs=3", "--subtract", "sleep 0.05", "sleep 0.2"]);
    let baseline = &results[0]["baseline"];
    assert!(baseline.get("net_mean_stderr").is_some());
    assert!(close(baseline["net_mean_stderr"].as_f64().unwrap(), 0.0));
}

#[test]
fn subtract_works_with_target_precision() {
    let results = results(&[
        "--min-runs=3",
        "--target-precision=5%",
        "--subtract",
        "sleep 0.05",
        "sleep 0.2",
    ]);
    assert!(close(results[0]["mean"].as_f64().unwrap(), 0.15));
    assert_eq!(results[0]["precision"]["met"], true);
    assert!(results[0]["baseline"].get("net_mean_stderr").is_some());
}

#[test]
fn subtract_works_with_compare() {
    let dir = tempfile::tempdir().unwrap();
    let base_json = dir.path().join("base.json");
    hyperfine()
        .args(["--debug-mode", "--style=basic", "--export-json"])
        .arg(&base_json)
        .args(["--runs=3", "--subtract", "sleep 0.05", "sleep 0.2"])
        .assert()
        .success();

    hyperfine()
        .args([
            "--debug-mode",
            "--style=basic",
            "--runs=3",
            "--subtract",
            "sleep 0.05",
            "--compare",
        ])
        .arg(&base_json)
        .arg("sleep 0.2")
        .assert()
        .success()
        .stdout(predicate::str::contains("sleep 0.2"))
        .stdout(predicate::str::contains("~"));
}

#[test]
fn subtract_help_mentions_energy() {
    hyperfine()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("'--energy' is used"));
}
