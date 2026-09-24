//! Tests for the per-run table exports.

mod common;
use common::hyperfine;

fn export(flag: &str, args: &[&str]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runs");
    hyperfine()
        .arg("--debug-mode")
        .args(args)
        .arg(flag)
        .arg(&path)
        .assert()
        .success();
    std::fs::read_to_string(&path).unwrap()
}

#[test]
fn markdown_runs_export_has_one_table_per_benchmark() {
    let out = export(
        "--export-markdown-runs",
        &["--runs=3", "sleep 0.1", "sleep 0.2"],
    );

    assert!(out.contains("### `sleep 0.1`"));
    assert!(out.contains("### `sleep 0.2`"));
    assert!(out.contains("| Iteration | Wall [ms] |"));
    // 3 runs per benchmark, numbered like JOULEX_ITERATION
    assert!(out.contains("| 0 | 100.0 |") && out.contains("| 2 | 100.0 |"));
    assert!(out.contains("| 0 | 200.0 |") && out.contains("| 2 | 200.0 |"));
    assert!(!out.contains("| 3 |"));
}

#[test]
fn orgmode_and_asciidoc_runs_exports_are_written() {
    let org = export("--export-orgmode-runs", &["--runs=2", "sleep 0.1"]);
    assert!(org.starts_with("* =sleep 0.1=\n"));
    assert!(org.contains("| Iteration  |"));

    let adoc = export("--export-asciidoc-runs", &["--runs=2", "sleep 0.1"]);
    assert!(adoc.starts_with("=== `sleep 0.1`\n"));
    assert!(adoc.contains("|==="));
}

#[cfg(unix)]
#[test]
fn omitted_runs_leave_gaps_in_the_iteration_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runs.md");
    hyperfine()
        .args([
            "-N",
            "--runs=4",
            "--ignore-failure",
            "--omit-failed-runs",
            "--export-markdown-runs",
        ])
        .arg(&path)
        .arg("sh -c '[ \"$JOULEX_ITERATION\" = 1 ] && exit 1; exit 0'")
        .assert()
        .success();
    let out = std::fs::read_to_string(&path).unwrap();

    let iterations: Vec<&str> = out
        .lines()
        .filter(|l| l.starts_with("| ") && !l.contains("Iteration"))
        .map(|l| l.split('|').nth(1).unwrap().trim())
        .collect();
    assert_eq!(iterations, vec!["0", "2", "3"]);
}
