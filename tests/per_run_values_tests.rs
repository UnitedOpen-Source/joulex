//! The JSON export and the per-run tables record which per-run parameter
//! value each run used (#192).

mod common;
use common::hyperfine;

use serde_json::Value;

fn export(args: &[&str]) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("out.json");
    hyperfine()
        .args(["--style=basic", "--export-json"])
        .arg(&json)
        .args(args)
        .assert()
        .success();
    let json: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    json["results"][0].clone()
}

#[test]
fn values_are_aligned_with_times_and_summarized() {
    let result = export(&[
        "--debug-mode",
        "--runs=4",
        "--first-run=discard",
        "--aggregate-parameter-runs",
        "-L",
        "t",
        "1,2",
        "sleep 0.{t}",
    ]);
    let times: Vec<f64> = serde_json::from_value(result["times"].clone()).unwrap();
    let values: Vec<String> =
        serde_json::from_value(result["per_run_parameters"]["values"]["t"].clone()).unwrap();
    assert_eq!(times.len(), values.len());
    for (time, value) in times.iter().zip(&values) {
        assert_eq!(format!("0.{value}"), format!("{time}"));
    }
    let stats = &result["per_run_parameters"]["stats"]["t"];
    assert_eq!(stats["1"]["mean"], 0.1);
    assert_eq!(stats["2"]["mean"], 0.2);
    assert_eq!(stats["2"]["runs"], 2);
}

#[test]
fn ordinary_benchmarks_have_no_per_run_values() {
    let result = export(&["--debug-mode", "--runs=2", "sleep 0.1"]);
    assert!(result.get("per_run_parameters").is_none());
}

/// Runs removed by --omit-failed-runs take their values with them.
#[cfg(unix)]
#[test]
fn omitted_runs_keep_the_values_aligned() {
    let result = export(&[
        "--runs=6",
        "-N",
        "--omit-failed-runs",
        "-i",
        "--aggregate-parameter-runs",
        "-L",
        "t",
        "1,2",
        "sh -c '[ {t} = 2 ] && exit 1; exit 0'",
    ]);
    let values: Vec<String> =
        serde_json::from_value(result["per_run_parameters"]["values"]["t"].clone()).unwrap();
    assert_eq!(values, ["1", "1", "1"]);
    assert_eq!(result["times"].as_array().unwrap().len(), 3);
}

#[test]
fn runs_table_has_a_column_per_parameter() {
    hyperfine()
        .args(["--debug-mode", "--runs=2", "--aggregate-parameter-runs"])
        .args([
            "-L",
            "t",
            "1,2",
            "--export-markdown-runs",
            "-",
            "sleep 0.{t}",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("| Iteration | t | Wall [ms] |"))
        .stdout(predicates::str::contains("| 0 | 1 | 100.0 |"))
        .stdout(predicates::str::contains("| 1 | 2 | 200.0 |"));
}
