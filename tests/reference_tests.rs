//! Tests for choosing the reference of the relative speed comparison.

mod common;
use common::hyperfine;

use predicates::prelude::*;

/// Mock executor: `sleep X` "takes" exactly X seconds.
fn hyperfine_debug() -> assert_cmd::Command {
    let mut cmd = hyperfine();
    cmd.arg("--debug-mode");
    cmd
}

fn write_import_file(
    dir: &std::path::Path,
    command: &str,
    mean: f64,
    exit_code: i32,
) -> std::path::PathBuf {
    let path = dir.join(format!("{}.json", command.replace(' ', "_")));
    std::fs::write(
        &path,
        format!(
            r#"{{"results":[{{"command":"{command}","mean":{mean},"stddev":0.001,"median":{mean},
            "user":0,"system":0,"min":{mean},"max":{mean},"times":[{mean}],"exit_codes":[{exit_code}]}}]}}"#
        ),
    )
    .unwrap();
    path
}

/// Regression test for #26: imported results come before the live benchmarks,
/// so the first result is not necessarily the reference.
#[test]
fn reference_is_used_even_when_results_were_imported() {
    let dir = tempfile::tempdir().unwrap();
    let imported = write_import_file(dir.path(), "imported slow", 5.0, 0);

    hyperfine_debug()
        .arg("--runs=2")
        .arg("--import-json")
        .arg(&imported)
        .arg("--reference=sleep 0.2")
        .arg("sleep 0.1")
        .assert()
        .success()
        .stdout(predicate::str::contains("Summary\n  sleep 0.2 ran"))
        .stdout(predicate::str::contains("times slower than sleep 0.1"))
        .stdout(predicate::str::contains("times faster than imported slow"));
}

/// The mock executor can't simulate failures, so this one runs real commands.
#[cfg(unix)]
#[test]
fn filtered_out_reference_falls_back_to_the_fastest_result() {
    let dir = tempfile::tempdir().unwrap();
    let imported = write_import_file(dir.path(), "imported fast", 0.001, 0);

    hyperfine()
        .arg("--shell=none")
        .arg("--runs=2")
        .arg("--import-json")
        .arg(&imported)
        .arg("--ignore-failure")
        .arg("--filter-failed")
        .arg("--reference=false")
        .arg("sleep 0.01")
        .assert()
        .success()
        .stdout(predicate::str::contains("Summary\n  imported fast ran"));
}

#[test]
fn without_reference_the_fastest_result_is_the_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let imported = write_import_file(dir.path(), "imported slow", 5.0, 0);

    hyperfine_debug()
        .arg("--runs=2")
        .arg("--import-json")
        .arg(&imported)
        .arg("sleep 0.1")
        .assert()
        .success()
        .stdout(predicate::str::contains("Summary\n  sleep 0.1 ran"));
}

#[test]
fn reference_is_respected_in_exports() {
    let dir = tempfile::tempdir().unwrap();
    let md_path = dir.path().join("out.md");
    let json_path = dir.path().join("out.json");
    let csv_path = dir.path().join("out.csv");
    let html_path = dir.path().join("out.html");

    hyperfine_debug()
        .arg("--runs=2")
        .arg("--reference=sleep 0.2")
        .arg("sleep 0.1")
        .arg("--export-markdown")
        .arg(&md_path)
        .arg("--export-json")
        .arg(&json_path)
        .arg("--export-csv")
        .arg(&csv_path)
        .arg("--export-html")
        .arg(&html_path)
        .assert()
        .success();

    let md = std::fs::read_to_string(&md_path).unwrap();
    assert!(md.contains("`sleep 0.2`"));
    assert!(md.contains("`sleep 0.1`"));
    for line in md.lines() {
        if line.contains("`sleep 0.2`") {
            assert!(
                line.contains("1.00"),
                "reference sleep 0.2 should have relative 1.00 in markdown, line: {line}"
            );
        } else if line.contains("`sleep 0.1`") {
            assert!(
                line.contains("2.00"),
                "sleep 0.1 should have relative 2.00 in markdown, line: {line}"
            );
        }
    }

    let json_str = std::fs::read_to_string(&json_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let results = parsed["results"].as_array().unwrap();
    let ref_entry = results
        .iter()
        .find(|r| r["command"] == "sleep 0.2")
        .unwrap();
    let fast_entry = results
        .iter()
        .find(|r| r["command"] == "sleep 0.1")
        .unwrap();
    assert_eq!(ref_entry["relative_speed"], 1.0);
    assert_eq!(fast_entry["relative_speed"], 2.0);

    let csv_str = std::fs::read_to_string(&csv_path).unwrap();
    for line in csv_str.lines() {
        if line.starts_with("sleep 0.2,") {
            assert!(line.contains(",1,"), "csv reference line: {line}");
        } else if line.starts_with("sleep 0.1,") {
            assert!(line.contains(",2,"), "csv fast line: {line}");
        }
    }

    let html_str = std::fs::read_to_string(&html_path).unwrap();
    assert!(html_str.contains("<code>sleep 0.2</code>"));
    assert!(html_str.contains("<code>sleep 0.1</code>"));
}
