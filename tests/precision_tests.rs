//! Tests for --precision.

mod common;
use common::hyperfine;

use predicates::prelude::*;

/// A result with mean 120.16350913 s and σ 1.488 s, re-exported to Markdown.
fn markdown(precision: Option<&str>) -> String {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    std::fs::write(
        &input,
        r#"{"results":[{"command":"build","mean":120.16350913,"stddev":1.488,"median":120.1,
            "user":1.0,"system":1.0,"min":118.76,"max":121.9,
            "times":[118.76,120.1,121.9],"exit_codes":[0,0,0]}]}"#,
    )
    .unwrap();
    let mut cmd = hyperfine();
    cmd.arg("--import-json")
        .arg(&input)
        .args(["--export-markdown", "-", "--style=none"]);
    if let Some(precision) = precision {
        cmd.arg(format!("--precision={precision}"));
    }
    let output = cmd.assert().success().get_output().stdout.clone();
    String::from_utf8(output).unwrap()
}

#[test]
fn default_precision_is_unchanged() {
    assert!(markdown(None).contains("| `build` | 120.164 ± 1.488 | 118.760 | 121.900 |"));
}

#[test]
fn fixed_precision() {
    assert!(markdown(Some("1")).contains("| `build` | 120.2 ± 1.5 | 118.8 | 121.9 |"));
    assert!(markdown(Some("0")).contains("| `build` | 120 ± 1 | 119 | 122 |"));
}

#[test]
fn auto_precision_uses_two_significant_digits_of_stddev() {
    // Mean and σ: 1 decimal (σ = 1.5); min/max keep the default decimals
    assert!(markdown(Some("auto")).contains("| `build` | 120.2 ± 1.5 | 118.760 | 121.900 |"));
}

#[test]
fn machine_formats_keep_full_precision() {
    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--precision=0",
            "--export-csv",
            "-",
        ])
        .arg("sleep 0.1234")
        .assert()
        .success()
        .stdout(predicate::str::contains("sleep 0.1234,0.1234,"));
}

#[test]
fn invalid_precision_is_rejected() {
    for value in ["10", "-1", "fast"] {
        hyperfine()
            .arg(format!("--precision={value}"))
            .arg("echo")
            .assert()
            .failure()
            .stderr(predicate::str::contains("expected a number of decimals"));
    }
}
