//! Tests for --shell given once per command (#67).

mod common;
use common::hyperfine;

use predicates::prelude::*;
use serde_json::Value;

fn shells_in_json(args: &[&str]) -> Vec<Option<String>> {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("out.json");
    hyperfine()
        .args(["--style=basic", "--export-json"])
        .arg(&json)
        .args(args)
        .assert()
        .success();
    let json: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["shell"].as_str().map(String::from))
        .collect()
}

#[test]
fn one_shell_for_all_commands_is_unchanged() {
    let shells = shells_in_json(&[
        "--debug-mode",
        "--runs=2",
        "-S",
        "sleep 0.01",
        "sleep 0.1",
        "sleep 0.2",
    ]);
    assert_eq!(shells, [None, None]);
}

#[test]
fn shells_per_command_are_recorded() {
    let shells = shells_in_json(&[
        "--debug-mode",
        "--runs=2",
        "-S",
        "sleep 0.01",
        "-S",
        "sleep 0.02",
        "sleep 0.1",
        "sleep 0.2",
    ]);
    assert_eq!(
        shells,
        [
            Some("sleep 0.01".to_string()),
            Some("sleep 0.02".to_string())
        ]
    );
}

#[test]
fn the_reference_counts_as_the_first_command() {
    hyperfine()
        .args(["--debug-mode", "--runs=2", "--reference", "sleep 0.1"])
        .args(["-S", "sleep 0.01", "-S", "sleep 0.02", "sleep 0.3"])
        .assert()
        .success();
}

#[test]
fn a_wrong_number_of_shells_is_rejected() {
    hyperfine()
        .args(["--debug-mode", "-S", "sleep 0.01", "-S", "sleep 0.02"])
        .args(["sleep 0.1", "sleep 0.2", "sleep 0.3"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The '--shell' option has to be provided just once or N times, where N=3",
        ));
}

#[cfg(unix)]
#[test]
fn each_command_runs_in_its_own_shell() {
    // `$0` is the shell's name; without a shell, `echo` prints '$0' literally
    hyperfine()
        .args(["--runs=1", "--show-output"])
        .args(["-S", "sh", "echo shell=$0", "-S", "none", "echo shell=$0"])
        .assert()
        .success()
        .stdout(predicate::str::contains("shell=sh"))
        .stdout(predicate::str::contains("shell=$0"));
}
