//! Tests for --aggregate-parameter-runs (#74) and --parameter-sample (#57).

mod common;
use common::hyperfine;

use predicates::prelude::*;
use serde_json::Value;

fn export(args: &[&str]) -> Vec<Value> {
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

fn times(result: &Value) -> Vec<f64> {
    result["times"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_f64().unwrap())
        .collect()
}

/// The upstream test (hyperfine#893): one run per value, pooled.
#[test]
fn aggregate_pools_all_values_into_one_result() {
    let results = export(&[
        "--runs=1",
        "--aggregate-parameter-runs",
        "-P",
        "i",
        "1",
        "3",
        "sleep {i}.123",
    ]);
    assert_eq!(results.len(), 1);
    assert_eq!(times(&results[0]), [1.123, 2.123, 3.123]);
    assert!((results[0]["mean"].as_f64().unwrap() - 2.123).abs() < 1e-9);
    assert!((results[0]["stddev"].as_f64().unwrap() - 1.0).abs() < 1e-9);
    assert!(results[0].get("parameters").is_none());
}

#[test]
fn aggregate_rounds_up_to_whole_cycles_per_template() {
    let results = export(&[
        "--runs=4",
        "--aggregate-parameter-runs",
        "-L",
        "t",
        "1,2,3",
        "sleep 0.{t}",
        "sleep 0.{t}5",
    ]);
    assert_eq!(results.len(), 2);
    assert_eq!(times(&results[0]), [0.1, 0.2, 0.3, 0.1, 0.2, 0.3]);
    assert_eq!(times(&results[1]), [0.15, 0.25, 0.35, 0.15, 0.25, 0.35]);
}

#[test]
fn aggregate_names_and_errors() {
    hyperfine()
        .args(["--debug-mode", "--runs=1", "--aggregate-parameter-runs"])
        .args(["-L", "t", "1,2", "sleep 0.{t}"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Benchmark 1: sleep 0.{t} (aggregated over 2 parameter values)",
        ));
    hyperfine()
        .args(["--debug-mode", "--aggregate-parameter-runs", "sleep 0.1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("needs parameters"));
}

#[test]
fn sample_gives_every_command_the_same_sequence() {
    let results = export(&[
        "--runs=30",
        "--parameter-sample",
        "t",
        "1,2,3",
        "sleep 0.{t}",
        "sleep 1.{t}",
    ]);
    let first: Vec<f64> = times(&results[0]);
    let second: Vec<f64> = times(&results[1]).iter().map(|t| t - 1.0).collect();
    assert_eq!(first.len(), 30);
    for (a, b) in first.iter().zip(&second) {
        assert!((a - b).abs() < 1e-9, "{first:?} vs {second:?}");
    }
    for value in [0.1, 0.2, 0.3] {
        assert!(first.contains(&value), "{value} never drawn: {first:?}");
    }
}

#[test]
fn seed_changes_the_sequence_reproducibly() {
    let run = |seed: &str| {
        times(
            &export(&[
                "--runs=20",
                "--seed",
                seed,
                "--parameter-sample",
                "t",
                "1,2,3,4",
                "sleep 0.{t}",
            ])[0],
        )
    };
    assert_eq!(run("5"), run("5"));
    assert_ne!(run("5"), run("6"));
}

#[test]
fn sample_combines_with_list_parameters() {
    let results = export(&[
        "--runs=10",
        "-L",
        "base",
        "1,2",
        "--parameter-sample",
        "t",
        "1,2",
        "sleep {base}.{t}",
    ]);
    assert_eq!(results.len(), 2);
    assert!(times(&results[0]).iter().all(|t| *t == 1.1 || *t == 1.2));
    assert!(times(&results[1]).iter().all(|t| *t == 2.1 || *t == 2.2));
}

#[test]
fn sample_parameter_name_errors() {
    hyperfine()
        .args([
            "--debug-mode",
            "--parameter-sample",
            "iteration",
            "1",
            "sleep 0.1",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
    hyperfine()
        .args([
            "--debug-mode",
            "-L",
            "t",
            "1",
            "--parameter-sample",
            "t",
            "2",
        ])
        .arg("sleep 0.{t}")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Duplicate parameter names: t"));
}

#[test]
fn setup_cannot_use_a_per_run_parameter() {
    for args in [
        ["--parameter-sample", "f", "a,b"],
        ["--aggregate-parameter-runs", "-L", "f"],
    ] {
        let mut cmd = hyperfine();
        cmd.args(["--debug-mode", "--setup", "echo {f}"]).args(args);
        if args[0] == "--aggregate-parameter-runs" {
            cmd.arg("a,b");
        }
        cmd.arg("sleep 0.1")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "cannot use the per-run parameter '{f}'",
            ));
    }
}
