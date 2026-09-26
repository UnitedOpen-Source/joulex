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
/// processes perfratio ever ran, so a small command benchmarked after a big one
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

fn export_json(args: &[&str]) -> serde_json::Value {
    let tempdir = tempfile::tempdir().unwrap();
    let export = tempdir.path().join("out.json");
    hyperfine()
        .args(args)
        .arg("--export-json")
        .arg(&export)
        .assert()
        .success();
    serde_json::from_str(&std::fs::read_to_string(&export).unwrap()).unwrap()
}

#[test]
fn resource_usage_line_is_printed() {
    hyperfine()
        .args([
            "-N",
            "--runs=3",
            "--style=basic",
            "--resource-usage",
            "true",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Resources (mean):   ctx-sw"))
        .stdout(predicates::str::contains("faults"))
        .stdout(predicates::str::contains("blocks"));
}

#[test]
fn resource_counters_are_exported_per_run() {
    let json = export_json(&["-N", "--runs=4", "--resource-usage", ALLOCATE_100MB]);
    let resources = &json["results"][0]["resources"];

    for key in [
        "voluntary_ctx_switches",
        "involuntary_ctx_switches",
        "minor_faults",
        "major_faults",
        "block_input_ops",
        "block_output_ops",
    ] {
        assert_eq!(resources[key].as_array().unwrap().len(), 4, "{key}");
    }
    // every process causes at least some page faults (the exact number depends
    // on the OS, e.g. transparent huge pages on Linux need far fewer faults)
    assert!(resources["minor_faults"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v.as_u64().unwrap() > 0));
}

#[test]
fn resource_counters_are_not_exported_by_default() {
    let json = export_json(&["-N", "--runs=2", "true"]);
    assert!(json["results"][0].get("resources").is_none());
}

#[test]
fn resource_counters_stay_aligned_with_omitted_runs() {
    let json = export_json(&[
        "-N",
        "--runs=5",
        "--ignore-failure",
        "--omit-failed-runs",
        "--resource-usage",
        "sh -c '[ \"$JOULEX_ITERATION\" = 2 ] && exit 1; true'",
    ]);
    let result = &json["results"][0];
    let runs = result["times"].as_array().unwrap().len();
    assert_eq!(runs, 4);
    assert_eq!(
        result["resources"]["minor_faults"]
            .as_array()
            .unwrap()
            .len(),
        runs
    );
}
