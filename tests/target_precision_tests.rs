//! Tests for --target-precision / --max-benchmarking-time.

mod common;
use common::hyperfine;

use predicates::prelude::*;
use serde_json::Value;

fn run_count(args: &[&str]) -> usize {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("out.json");
    hyperfine()
        .args(["--style=basic", "--export-json"])
        .arg(&json)
        .args(args)
        .assert()
        .success();
    let json: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    json["results"][0]["times"].as_array().unwrap().len()
}

#[test]
fn a_precise_command_stops_at_the_minimum_number_of_runs() {
    // The mock's times are constant: the CI is 0 as soon as there are 2 runs
    assert_eq!(
        run_count(&["--debug-mode", "--target-precision=1%", "sleep 0.1"]),
        10
    );
    assert_eq!(
        run_count(&[
            "--debug-mode",
            "--target-precision=1%",
            "--min-runs=3",
            "sleep 0.1"
        ]),
        3
    );
}

#[test]
fn the_precision_reached_is_reported() {
    hyperfine()
        .args(["--debug-mode", "--target-precision=1%", "sleep 0.1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("10 runs (±0.0% @95%, target 1%)"));
}

#[test]
fn aggregated_runs_stop_only_after_whole_cycles() {
    let runs = run_count(&[
        "--debug-mode",
        "--target-precision=1%",
        "--min-runs=4",
        "--aggregate-parameter-runs",
        "-L",
        "t",
        "1,2,3",
        "sleep 0.{t}",
    ]);
    assert_eq!(runs % 3, 0, "{runs}");
    assert!(runs >= 4);
}

#[test]
fn invalid_values_are_rejected() {
    for value in ["0%", "-1", "fast"] {
        hyperfine()
            .arg(format!("--target-precision={value}"))
            .arg("echo")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Invalid '--target-precision'"));
    }
    hyperfine()
        .args(["--target-precision=1%", "--runs=5", "echo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
    hyperfine()
        .args(["--max-benchmarking-time=5", "echo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--target-precision"));
}

#[cfg(unix)]
mod unix {
    use super::*;

    /// 10–50 ms at random: a 0.1% target can't be reached in a few runs.
    const NOISY: &str = "sleep 0.0$(( $(od -An -N1 -tu1 /dev/urandom) % 5 + 1 ))";

    #[test]
    fn max_runs_bounds_the_runs_and_warns() {
        hyperfine()
            .args([
                "--target-precision=0.1%",
                "--max-runs=12",
                "--style=basic",
                NOISY,
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("12 runs (±"))
            .stderr(predicate::str::contains(
                "The target precision of ±0.1% was not reached after 12 runs",
            ));
    }

    #[test]
    fn the_time_budget_bounds_the_runs() {
        let started = std::time::Instant::now();
        hyperfine()
            .args(["--target-precision=0.01%", "--max-benchmarking-time=1"])
            .args(["--style=basic", NOISY])
            .assert()
            .success()
            .stderr(predicate::str::contains("was not reached"));
        assert!(started.elapsed().as_secs_f64() < 15.0);
    }

    #[test]
    fn round_robin_stops_each_command_at_its_target() {
        hyperfine()
            .args([
                "--target-precision=0.1%",
                "--max-runs=11",
                "--schedule=round-robin",
            ])
            .args(["--style=basic", NOISY, "sleep 0.01"])
            .assert()
            .success()
            .stdout(predicate::str::contains("11 runs (±").count(2));
    }
}
