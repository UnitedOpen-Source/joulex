//! A closed stdout (e.g. `joulex … | head -1`) must not abort joulex or lose
//! its exports (#187).

// Only the raw command builder is used here (and nothing on Windows)
#[allow(dead_code)]
mod common;

#[cfg(unix)]
#[test]
fn closed_stdout_neither_panics_nor_loses_exports() {
    use std::process::Stdio;

    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("out.json");
    let mut child = common::hyperfine_raw_command()
        .args(["--debug-mode", "--runs=3", "--style=basic", "--export-json"])
        .arg(&json)
        .args(["sleep 0.1", "sleep 0.2"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Close the read end before joulex writes anything
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{:?}: {stderr}", output.status);
    assert!(!stderr.contains("panicked"), "{stderr}");

    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(json["results"].as_array().unwrap().len(), 2);
}
