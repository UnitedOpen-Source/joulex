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
