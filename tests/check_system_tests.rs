//! Tests for --check-system, using a fake Linux /sys and /proc tree (the
//! hidden JOULEX_SYSTEM_CHECK_ROOT hook), so they run on every platform.

mod common;
use common::hyperfine;

use predicates::prelude::*;

/// A fake root with one CPU using `governor` and an idle load.
fn fake_root(governor: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let write = |path: &str, content: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    };
    write(
        "sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
        &format!("{governor}\n"),
    );
    write("sys/devices/system/cpu/intel_pstate/no_turbo", "1\n");
    write("proc/loadavg", "0.01 0.01 0.01 1/10 1\n");
    dir
}

#[test]
fn reports_problems_with_hints_and_continues() {
    let root = fake_root("powersave");
    hyperfine()
        .env("JOULEX_SYSTEM_CHECK_ROOT", root.path())
        .args(["--check-system", "--debug-mode", "--runs=1", "sleep 0.1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("System check:"))
        .stdout(predicate::str::contains(
            "CPU governor     powersave on 1 CPU",
        ))
        .stdout(predicate::str::contains(
            "sudo cpupower frequency-set -g performance",
        ))
        .stdout(predicate::str::contains("Turbo boost      disabled"))
        .stdout(predicate::str::contains("Benchmark 1: sleep 0.1"));
}

#[test]
fn strict_mode_aborts_with_exit_code_4() {
    let root = fake_root("powersave");
    hyperfine()
        .env("JOULEX_SYSTEM_CHECK_ROOT", root.path())
        .args([
            "--check-system=strict",
            "--debug-mode",
            "--runs=1",
            "sleep 0.1",
        ])
        .assert()
        .code(4)
        .stdout(predicate::str::contains("Benchmark 1").not())
        .stderr(predicate::str::contains(
            "did not pass ('--check-system=strict')",
        ));
}

#[test]
fn strict_mode_passes_on_a_quiet_system() {
    let root = fake_root("performance");
    hyperfine()
        .env("JOULEX_SYSTEM_CHECK_ROOT", root.path())
        .args([
            "--check-system=strict",
            "--debug-mode",
            "--runs=1",
            "sleep 0.1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Benchmark 1: sleep 0.1"));
}

#[test]
fn strict_failure_is_explained_even_without_output() {
    let root = fake_root("powersave");
    hyperfine()
        .env("JOULEX_SYSTEM_CHECK_ROOT", root.path())
        .args([
            "--check-system=strict",
            "--style=none",
            "--debug-mode",
            "sleep 0.1",
        ])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("CPU governor"));
}

#[test]
fn without_the_option_nothing_is_checked() {
    let root = fake_root("powersave");
    hyperfine()
        .env("JOULEX_SYSTEM_CHECK_ROOT", root.path())
        .args(["--debug-mode", "--runs=1", "sleep 0.1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("System check").not());
}

#[test]
fn json_export_records_the_environment() {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("out.json");
    hyperfine()
        .args(["--debug-mode", "--runs=1", "--export-json"])
        .arg(&json)
        .arg("sleep 0.1")
        .assert()
        .success();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    let system = &json["perfratio"]["system"];
    assert_eq!(system, &json["joulex"]["system"]);
    assert!(system["cpus"].as_u64().unwrap() >= 1);
    if cfg!(unix) {
        assert!(!system["kernel"].as_str().unwrap().is_empty());
    }
    if cfg!(target_os = "macos") {
        assert!(!system["cpu_model"].as_str().unwrap().is_empty());
    }
}
