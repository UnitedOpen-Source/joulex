//! Tests for --expand-used-parameters.

mod common;
use common::hyperfine;

#[cfg(unix)]
use predicates::prelude::*;

fn benchmark_lines(output: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(output)
        .lines()
        .filter(|l| l.starts_with("Benchmark "))
        .map(String::from)
        .collect()
}

#[test]
fn commands_are_only_combined_with_the_parameters_they_use() {
    let args = [
        "--debug-mode",
        "--runs=1",
        "--style=basic",
        "-L",
        "A",
        "1,2",
        "-L",
        "B",
        "x,y",
        "-L",
        "C",
        "p,q",
        "sleep 0.1",
        "sleep 0.2",
        "sleep 0.{A}",
    ];

    // default: every command × every combination = 3 × 8 = 24 benchmarks
    let output = hyperfine().args(args).output().unwrap();
    assert_eq!(benchmark_lines(&output.stdout).len(), 24);

    // with the flag: 1 + 1 + 2 = 4 benchmarks
    let output = hyperfine()
        .arg("--expand-used-parameters")
        .args(args)
        .output()
        .unwrap();
    assert_eq!(
        benchmark_lines(&output.stdout),
        vec![
            "Benchmark 1: sleep 0.1",
            "Benchmark 2: sleep 0.2",
            "Benchmark 3: sleep 0.1",
            "Benchmark 4: sleep 0.2",
        ]
    );
}

#[cfg(unix)]
#[test]
fn parameters_used_in_prepare_still_create_distinct_benchmarks() {
    hyperfine()
        .args([
            "-N",
            "--runs=1",
            "--style=basic",
            "--expand-used-parameters",
        ])
        .args([
            "-L",
            "A",
            "1,2",
            "-L",
            "B",
            "x,y",
            "--prepare=echo {B}",
            "true",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Benchmark 1: true (B = x)"))
        .stdout(predicate::str::contains("Benchmark 2: true (B = y)"))
        .stdout(predicate::str::contains("Benchmark 3").not());
}

#[cfg(unix)]
#[test]
fn parameters_used_in_the_command_name_still_create_distinct_benchmarks() {
    hyperfine()
        .args([
            "-N",
            "--runs=1",
            "--style=basic",
            "--expand-used-parameters",
        ])
        .args(["-L", "A", "1,2", "-L", "B", "x,y", "-n", "run-{A}", "true"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Benchmark 1: run-1"))
        .stdout(predicate::str::contains("Benchmark 2: run-2"))
        .stdout(predicate::str::contains("Benchmark 3").not());
}

#[test]
fn exports_only_contain_the_used_parameters() {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("out.json");
    hyperfine()
        .args(["--debug-mode", "--runs=1", "--expand-used-parameters"])
        .args(["-L", "A", "1,2", "-L", "B", "x,y", "--export-json"])
        .arg(&json)
        .args(["sleep 0.{A}", "sleep 0.5"])
        .assert()
        .success();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    let results = json["results"].as_array().unwrap();

    assert_eq!(results.len(), 3);
    let parameters_of = |command: &str| {
        results
            .iter()
            .find(|r| r["command"] == command)
            .map(|r| r.get("parameters").cloned())
            .unwrap()
    };
    assert_eq!(
        parameters_of("sleep 0.1"),
        Some(serde_json::json!({"A": "1"}))
    );
    assert_eq!(
        parameters_of("sleep 0.2"),
        Some(serde_json::json!({"A": "2"}))
    );
    // `B` is not used by any command, and `sleep 0.5` uses no parameter at all
    assert_eq!(parameters_of("sleep 0.5"), None);
}
