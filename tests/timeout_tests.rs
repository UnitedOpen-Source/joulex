//! Tests for --timeout (#51).

mod common;
use common::hyperfine;

use predicates::prelude::*;
use serde_json::Value;

#[test]
fn invalid_timeout_durations_are_rejected() {
    for invalid in ["0", "0s", "0ms", "-1s", "abc", "inf", "NaN", "-500ms"] {
        hyperfine()
            .args(["--timeout", invalid, "sleep 0.01"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Invalid duration"));
    }
}

#[test]
fn timed_out_benchmark_skips_remaining_runs_and_reports_timeout() {
    let start = std::time::Instant::now();
    hyperfine()
        .args([
            "--timeout",
            "150ms",
            "-N",
            "--runs=3",
            "--style=basic",
            "sleep 5",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Timed out"))
        .stdout(predicate::str::contains("skipped remaining runs"));

    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "Timed out benchmark took too long: {:?}",
        start.elapsed()
    );
}

#[test]
fn timeout_with_multiple_commands_and_relative_speed() {
    let start = std::time::Instant::now();
    hyperfine()
        .args([
            "--timeout",
            "150ms",
            "-N",
            "--runs=2",
            "--style=basic",
            "sleep 5",
            "sleep 0.01",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Timed out"))
        .stdout(predicate::str::contains("(timeout)"))
        .stdout(predicate::str::contains("sleep 0.01"));

    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "Multiple command benchmark took too long: {:?}",
        start.elapsed()
    );
}

#[test]
fn timeout_json_export() {
    let dir = tempfile::tempdir().unwrap();
    let json_file = dir.path().join("out.json");

    hyperfine()
        .args(["--timeout", "150ms", "-N", "--runs=2", "--export-json"])
        .arg(&json_file)
        .args(["sleep 5", "sleep 0.01"])
        .assert()
        .success();

    let content = std::fs::read_to_string(&json_file).unwrap();
    let json: Value = serde_json::from_str(&content).unwrap();
    let results = json["results"].as_array().unwrap();

    assert_eq!(results.len(), 2);

    // Command 1: timed out on first run
    let cmd1 = &results[0];
    assert_eq!(cmd1["command"], "sleep 5");
    assert_eq!(cmd1["timed_out"], true);
    assert!((cmd1["timeout"].as_f64().unwrap() - 0.15).abs() < 1e-6);
    assert_eq!(cmd1["times"].as_array().unwrap().len(), 0);

    // Command 2: completed normally
    let cmd2 = &results[1];
    assert_eq!(cmd2["command"], "sleep 0.01");
    assert!(cmd2.get("timed_out").is_none() || cmd2["timed_out"] == false);
    assert_eq!(cmd2["times"].as_array().unwrap().len(), 2);
}

#[test]
fn timeout_csv_and_markdown_exports() {
    let dir = tempfile::tempdir().unwrap();
    let csv_file = dir.path().join("out.csv");
    let md_file = dir.path().join("out.md");

    hyperfine()
        .args(["--timeout", "200ms", "-N", "--runs=2", "--export-csv"])
        .arg(&csv_file)
        .arg("--export-markdown")
        .arg(&md_file)
        .args(["sleep 5", "sleep 0.01"])
        .assert()
        .success();

    // Verify CSV output
    let csv_content = std::fs::read_to_string(&csv_file).unwrap();
    assert!(
        csv_content.contains(">0.200 (timeout)"),
        "CSV should contain '>0.200 (timeout)':\n{csv_content}"
    );

    // Verify Markdown output
    let md_content = std::fs::read_to_string(&md_file).unwrap();
    assert!(
        md_content.contains("(timeout)"),
        "Markdown should contain '(timeout)':\n{md_content}"
    );
    assert!(
        md_content.contains("n/a"),
        "Markdown relative column should be 'n/a' for timed out command:\n{md_content}"
    );
}

#[test]
fn intermediate_preparation_timeout_aborts_with_error() {
    hyperfine()
        .args(["--timeout", "150ms", "--prepare", "sleep 5", "sleep 0.01"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The preparation command timed out",
        ));
}

#[test]
fn intermediate_setup_timeout_aborts_with_error() {
    hyperfine()
        .args(["--timeout", "150ms", "--setup", "sleep 5", "sleep 0.01"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("The setup command timed out"));
}

#[cfg(unix)]
#[test]
fn process_group_kills_descendants_on_unix() {
    // Spawn a subshell that launches a background sleep and waits
    // Watchdog should kill the entire process group including the background sleep
    let start = std::time::Instant::now();
    hyperfine()
        .args(["--timeout", "200ms", "--runs=1", "sh -c 'sleep 10 & wait'"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Timed out"));

    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "Process tree kill took too long: {:?}",
        start.elapsed()
    );
}
