//! Tests for `--warmup auto`.

mod common;
use common::hyperfine;

use predicates::prelude::*;

#[test]
fn auto_warmup_stops_as_soon_as_timings_are_stable() {
    let dir = tempfile::tempdir().unwrap();
    let export = dir.path().join("out.json");
    hyperfine()
        .args(["--debug-mode", "--runs=2", "--style=basic", "--warmup=auto"])
        .arg("--export-json")
        .arg(&export)
        .arg("sleep 0.1")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Warmup (auto):      5 runs, last 5 within 0.0%",
        ));

    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&export).unwrap()).unwrap();
    assert_eq!(json["results"][0]["warmup_runs"], 5);
}

#[test]
fn fixed_warmup_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let export = dir.path().join("out.json");
    hyperfine()
        .args(["--debug-mode", "--runs=2", "--style=basic", "--warmup=3"])
        .arg("--export-json")
        .arg(&export)
        .arg("sleep 0.1")
        .assert()
        .success()
        .stdout(predicate::str::contains("Warmup (auto)").not());

    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&export).unwrap()).unwrap();
    assert!(json["results"][0].get("warmup_runs").is_none());
}

#[test]
fn auto_warmup_works_with_round_robin() {
    hyperfine()
        .args(["--debug-mode", "--runs=2", "--style=basic", "--warmup=auto"])
        .args(["--schedule=round-robin", "sleep 0.1", "sleep 0.2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Warmup (auto)").count(2));
}

#[cfg(unix)]
#[test]
fn auto_warmup_gives_up_after_100_runs_with_a_warning() {
    // Alternates between ~1 ms and ~15 ms, so the timings never stabilize.
    hyperfine()
        .args(["-N", "--runs=2", "--style=basic", "--warmup=auto"])
        .arg("sh -c 'case $JOULEX_ITERATION in *[13579]) sleep 0.015;; esac; true'")
        .assert()
        .success()
        .stdout(predicate::str::contains("100 runs"))
        .stdout(predicate::str::contains("(not stable)"))
        .stderr(predicate::str::contains(
            "'--warmup auto' stopped after 100 runs without the timings becoming stable",
        ));
}

#[test]
fn invalid_warmup_values_are_rejected() {
    hyperfine()
        .args(["--debug-mode", "--warmup=abc", "sleep 0.1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("'--warmup'"));
}
