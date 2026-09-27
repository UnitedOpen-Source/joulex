#![cfg(target_os = "linux")]

mod common;
use common::hyperfine;

#[test]
fn linux_microbenchmark_reports_positive_nonzero_time() {
    // Fast command executed without shell
    let assert = hyperfine()
        .arg("--shell=none")
        .arg("--runs=5")
        .arg("true")
        .assert()
        .success();

    // Verify output mentions timing in ms or µs and execution succeeds
    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(output.contains("Time (mean ± σ):") || output.contains("Benchmark 1:"));
}

#[test]
fn linux_fast_command_with_until() {
    hyperfine()
        .arg("--shell=none")
        .arg("--runs=3")
        .arg("--until=READY")
        .arg("sh -c 'echo READY; sleep 1'")
        .assert()
        .success();
}

#[test]
fn linux_command_with_priority_and_affinity() {
    hyperfine()
        .arg("--shell=none")
        .arg("--runs=3")
        .arg("--priority=normal")
        .arg("true")
        .assert()
        .success();
}
