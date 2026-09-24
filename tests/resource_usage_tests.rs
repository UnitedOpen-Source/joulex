//! Tests for per-run resource usage measurement (user/system time, peak memory).

#![cfg(unix)]

mod common;
use common::hyperfine;

fn exported_memory(args: &[&str]) -> Vec<Vec<u64>> {
    let tempdir = tempfile::tempdir().unwrap();
    let export = tempdir.path().join("out.json");
    hyperfine()
        .args(args)
        .arg("--export-json")
        .arg(&export)
        .assert()
        .success();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&export).unwrap()).unwrap();
    json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            r["memory_usage_byte"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect()
        })
        .collect()
}

const MB: u64 = 1024 * 1024;
const ALLOCATE_100MB: &str = "dd if=/dev/zero of=/dev/null bs=104857600 count=1";

/// Regression test for #46: peak memory used to be the maximum over *all*
/// processes joulex ever ran, so a small command benchmarked after a big one
/// reported the big one's peak.
#[test]
fn peak_memory_does_not_leak_between_benchmarks() {
    let memory = exported_memory(&["-N", "--runs=2", ALLOCATE_100MB, "true"]);

    assert!(memory[0].iter().all(|&m| m > 90 * MB), "{memory:?}");
    assert!(memory[1].iter().all(|&m| m < 50 * MB), "{memory:?}");
}

#[test]
fn peak_memory_does_not_include_prepare_commands() {
    let memory = exported_memory(&[
        "-N",
        "--runs=2",
        &format!("--prepare={ALLOCATE_100MB}"),
        "true",
    ]);

    assert!(memory[0].iter().all(|&m| m < 50 * MB), "{memory:?}");
}

#[test]
fn peak_memory_includes_children_of_the_intermediate_shell() {
    // With the default shell, the measured process is `sh -c '<command>'`;
    // the allocation happens in a grandchild that the shell waits for.
    let memory = exported_memory(&["--runs=2", &format!("{ALLOCATE_100MB}; true")]);

    assert!(memory[0].iter().all(|&m| m > 90 * MB), "{memory:?}");
}
