//! Tests for the run metadata, labels and relative speed in JSON/CSV exports.

mod common;
use common::hyperfine;

use predicates::prelude::*;

fn export(args: &[&str]) -> (serde_json::Value, String) {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("out.json");
    let csv = dir.path().join("out.csv");
    hyperfine()
        .arg("--debug-mode")
        .args(args)
        .arg("--export-json")
        .arg(&json)
        .arg("--export-csv")
        .arg(&csv)
        .assert()
        .success();
    (
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap(),
        std::fs::read_to_string(&csv).unwrap(),
    )
}

#[test]
fn json_contains_run_metadata_and_labels() {
    let (json, _) = export(&[
        "--label",
        "commit=abc123",
        "--label",
        "runner=ci",
        "sleep 0.1",
    ]);
    let meta = &json["perfratio"];
    assert_eq!(meta, &json["joulex"]);

    assert_eq!(meta["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(meta["labels"]["commit"], "abc123");
    assert_eq!(meta["labels"]["runner"], "ci");
    assert_eq!(meta["system"]["os"], std::env::consts::OS);
    assert!(meta["started_at"].as_str().unwrap().ends_with('Z'));
    // the program path is reduced to its file name (no home directory leak)
    let program = meta["command_line"][0].as_str().unwrap();
    assert!(
        !program.contains('/') && !program.contains('\\'),
        "{program}"
    );
}

#[test]
fn json_contains_relative_speed_to_the_fastest_result() {
    let (json, _) = export(&["sleep 0.2", "sleep 0.1"]);
    let results = json["results"].as_array().unwrap();

    assert!((results[0]["relative_speed"].as_f64().unwrap() - 2.0).abs() < 1e-9);
    assert!((results[1]["relative_speed"].as_f64().unwrap() - 1.0).abs() < 1e-9);
    // the baseline has no spread relative to itself
    assert!(results[1].get("relative_speed_stddev").is_none());
}

#[test]
fn csv_appends_relative_speed_and_label_columns() {
    let (_, csv) = export(&["--label", "commit=abc123", "sleep 0.2", "sleep 0.1"]);
    let mut lines = csv.lines();

    assert_eq!(
        lines.next().unwrap(),
        "command,mean,stddev,median,user,system,min,max,relative_speed,relative_speed_stddev,label_commit"
    );
    assert!(lines.next().unwrap().ends_with(",abc123"));
}

#[test]
fn new_json_exports_can_be_imported_again() {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("out.json");
    hyperfine()
        .args(["--debug-mode", "--label", "k=v", "--export-json"])
        .arg(&json)
        .arg("sleep 0.1")
        .assert()
        .success();

    hyperfine()
        .arg("--import-json")
        .arg(&json)
        .assert()
        .success()
        .stdout(predicate::str::contains("sleep 0.1 (imported)"));
}

#[test]
fn rejects_invalid_labels() {
    for label in ["novalue", "bad key=1"] {
        hyperfine()
            .args(["--debug-mode", "--label", label, "sleep 0.1"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("Invalid label"));
    }
    hyperfine()
        .args([
            "--debug-mode",
            "--label",
            "a=1",
            "--label",
            "a=2",
            "sleep 0.1",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Duplicate label key 'a'"));
}

#[test]
fn json_contains_percentiles_and_geometric_mean() {
    let (json, _) = export(&["--runs=5", "sleep 0.1"]);
    let result = &json["results"][0];

    for key in ["p05", "p25", "p75", "p95"] {
        let value = result["percentiles"][key].as_f64().unwrap();
        assert!((value - 0.1).abs() < 1e-9, "{key}: {value}");
    }
    assert!((result["geometric_mean"].as_f64().unwrap() - 0.1).abs() < 1e-9);
}

#[test]
fn json_percentiles_are_imported_back_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.json");
    let second = dir.path().join("second.json");
    hyperfine()
        .args(["--debug-mode", "--runs=5", "--export-json"])
        .arg(&first)
        .arg("sleep 0.1")
        .assert()
        .success();
    hyperfine()
        .arg("--import-json")
        .arg(&first)
        .arg("--export-json")
        .arg(&second)
        .assert()
        .success();

    let read = |p: &std::path::Path| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    };
    assert_eq!(
        read(&first)["results"][0]["percentiles"],
        read(&second)["results"][0]["percentiles"]
    );
}
