mod common;
use common::hyperfine;

fn html_report(args: &[&str], file_name: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(file_name);
    hyperfine()
        .args(["--debug-mode", "--runs=5"])
        .args(args)
        .arg(&path)
        .args(["sleep 0.1", "sleep 0.2"])
        .assert()
        .success();
    std::fs::read_to_string(&path).unwrap()
}

#[test]
fn export_html_writes_a_self_contained_report() {
    let html = html_report(&["--export-html"], "report.html");

    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("sleep 0.1"));
    assert!(html.contains("sleep 0.2"));
    assert!(html.contains("Histogram of run times"));
    assert!(html.contains("Run times in run order"));
    assert!(html.contains("Box plot of all commands"));
    assert!(!html.contains("<script"));
    assert!(!html.contains("http://") && !html.contains("src=\"https://"));
    assert!(!html.contains("NaN") && !html.contains("inf"));
}

#[test]
fn export_detects_html_from_the_file_extension() {
    let html = html_report(&["--export"], "report.htm");
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("Box plot of all commands"));
}
