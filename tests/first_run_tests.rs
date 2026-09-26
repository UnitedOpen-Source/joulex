//! Tests for --first-run include|separate|discard.

mod common;
use common::hyperfine;

use predicates::prelude::*;
use serde_json::Value;

/// Run perfratio with `args` and the JSON export, and return (stdout, results).
fn run(args: &[&str]) -> (String, Vec<Value>) {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("out.json");
    let output = hyperfine()
        .args(["--style=basic", "--export-json"])
        .arg(&json)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    (
        String::from_utf8(output).unwrap(),
        json["results"].as_array().unwrap().clone(),
    )
}

fn run_count(result: &Value) -> usize {
    result["times"].as_array().unwrap().len()
}

#[test]
fn include_is_the_default() {
    let (stdout, results) = run(&["--debug-mode", "--runs=3", "sleep 0.1"]);
    assert_eq!(run_count(&results[0]), 3);
    assert!(results[0].get("first_run").is_none());
    assert!(!stdout.contains("Cold"));
}

#[test]
fn separate_reports_the_first_run_and_still_measures_the_requested_runs() {
    let (stdout, results) = run(&[
        "--debug-mode",
        "--runs=3",
        "--first-run=separate",
        "sleep 0.1",
    ]);
    assert_eq!(run_count(&results[0]), 3);
    assert_eq!(results[0]["first_run"]["time"], 0.1);
    assert!(results[0]["first_run"].get("warmup").is_none());
    assert!(stdout.contains("Cold (1st run):"));
    assert!(stdout.contains("3 runs (+1 cold)"));
}

#[test]
fn discard_drops_the_first_run_silently() {
    let (stdout, results) = run(&[
        "--debug-mode",
        "--runs=3",
        "--first-run=discard",
        "sleep 0.1",
    ]);
    assert_eq!(run_count(&results[0]), 3);
    assert!(results[0].get("first_run").is_none());
    assert!(!stdout.contains("Cold"));
    assert!(stdout.contains("3 runs (first run discarded)"));
}

#[test]
fn separate_with_warmup_reports_the_first_warmup_run() {
    let (stdout, results) = run(&[
        "--debug-mode",
        "--runs=3",
        "--warmup=2",
        "--first-run=separate",
        "sleep 0.1",
    ]);
    assert_eq!(run_count(&results[0]), 3);
    assert_eq!(results[0]["first_run"]["warmup"], true);
    assert_eq!(results[0]["first_run"]["time"], 0.1);
    assert!(stdout.contains("Cold (warmup 1):"));
    assert!(!stdout.contains("+1 cold"));
}

#[test]
fn separate_with_a_single_run() {
    let (_, results) = run(&[
        "--debug-mode",
        "--runs=1",
        "--first-run=separate",
        "sleep 0.1",
    ]);
    assert_eq!(run_count(&results[0]), 1);
    assert!(results[0]["first_run"].is_object());
}

#[test]
fn separate_in_round_robin_mode() {
    let (_, results) = run(&[
        "--debug-mode",
        "--runs=3",
        "--schedule=round-robin",
        "--first-run=separate",
        "sleep 0.1",
        "sleep 0.2",
    ]);
    for (result, time) in results.iter().zip([0.1, 0.2]) {
        assert_eq!(run_count(result), 3);
        assert_eq!(result["first_run"]["time"], time);
    }
}

#[test]
fn invalid_mode_is_rejected() {
    hyperfine()
        .args(["--first-run=cold", "echo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value 'cold'"));
}

/// A real cold start: the first run creates a marker file and is slow, the
/// following runs are fast.
#[cfg(unix)]
#[test]
fn separate_excludes_a_real_cold_start_from_the_statistics() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("warm");
    let marker = marker.to_str().unwrap();
    let command =
        format!("if [ -e '{marker}' ]; then sleep 0.01; else touch '{marker}'; sleep 0.5; fi");
    let (_, results) = run(&["--runs=3", "--first-run=separate", &command]);

    let cold = results[0]["first_run"]["time"].as_f64().unwrap();
    let max = results[0]["max"].as_f64().unwrap();
    assert!(cold >= 0.5, "cold run: {cold}");
    assert!(
        max < 0.4,
        "warm runs should exclude the cold one: max {max}"
    );
}
