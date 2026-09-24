//! Tests for per-command --setup and --cleanup.

#![cfg(unix)]

mod common;
use common::hyperfine;

use predicates::prelude::*;

#[test]
fn each_command_runs_its_own_setup_and_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log");
    let log = log.to_str().unwrap();

    hyperfine()
        .args(["--runs=2", "-N"])
        .arg(format!("--setup=sh -c 'echo setup-a >> {log}'"))
        .arg(format!("--setup=sh -c 'echo setup-b >> {log}'"))
        .arg(format!("--cleanup=sh -c 'echo cleanup-a >> {log}'"))
        .arg(format!("--cleanup=sh -c 'echo cleanup-b >> {log}'"))
        .arg(format!("sh -c 'echo run-a >> {log}'"))
        .arg(format!("sh -c 'echo run-b >> {log}'"))
        .assert()
        .success();

    let mut sequence: Vec<String> = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(String::from)
        .collect();
    sequence.dedup(); // collapse the repeated timing runs
    assert_eq!(
        sequence,
        vec![
            "setup-a",
            "run-a",
            "cleanup-a",
            "setup-b",
            "run-b",
            "cleanup-b"
        ]
    );
}

#[test]
fn setup_given_once_applies_to_all_commands() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log");
    let log = log.to_str().unwrap();

    hyperfine()
        .args(["--runs=1", "-N"])
        .arg(format!("--setup=sh -c 'echo setup >> {log}'"))
        .arg("true")
        .arg("false || true")
        .arg("--ignore-failure")
        .assert()
        .success();

    let setups = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .filter(|l| *l == "setup")
        .count();
    assert_eq!(setups, 2);
}

#[test]
fn wrong_number_of_setup_or_cleanup_options_is_rejected() {
    for flag in ["--setup", "--cleanup"] {
        hyperfine()
            .args(["--runs=1", "-N"])
            .arg(format!("{flag}=true"))
            .arg(format!("{flag}=true"))
            .args(["true", "true", "true"])
            .assert()
            .failure()
            .stderr(predicate::str::contains(format!(
                "The '{flag}' option has to be provided just once or N times, where N=3"
            )));
    }
}

#[test]
fn round_robin_rejects_different_per_command_setups() {
    hyperfine()
        .args(["--runs=1", "-N", "--schedule=round-robin"])
        .args(["--setup=echo a", "--setup=echo b"])
        .args(["true", "true"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot be combined with '--schedule round-robin'",
        ));
}

#[test]
fn round_robin_allows_identical_per_command_setups() {
    hyperfine()
        .args(["--runs=1", "-N", "--schedule=round-robin"])
        .args(["--setup=true", "--setup=true"])
        .args(["true", "true"])
        .assert()
        .success();
}
