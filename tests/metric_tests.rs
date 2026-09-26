mod common;
use common::hyperfine;

use predicates::prelude::*;

#[test]
fn metric_rejects_invalid_value() {
    hyperfine()
        .args(["--debug-mode", "--metric=invalid", "sleep 0.1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value 'invalid'"));
}

#[test]
fn metric_accepts_all_valid_variants_and_aliases() {
    for metric in [
        "wall",
        "wall-clock",
        "time",
        "cpu",
        "total-cpu",
        "user",
        "system",
        "memory",
        "rss",
        "peak-memory",
    ] {
        hyperfine()
            .args([
                "--debug-mode",
                "--runs=2",
                &format!("--metric={metric}"),
                "sleep 0.01",
            ])
            .assert()
            .success();
    }
}

#[test]
fn metric_cpu_displays_cpu_header_and_time_secondary() {
    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--metric=cpu",
            "sleep 0.1 0.06 0.04",
            "sleep 0.2 0.12 0.08",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("CPU Time (mean ± σ):"))
        .stdout(predicate::str::contains("Time (mean ± σ):"))
        .stdout(predicate::str::contains("ran"))
        .stdout(predicate::str::contains("faster than"));
}

#[test]
fn metric_memory_displays_memory_header_and_used_less_memory() {
    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--metric=memory",
            "sleep 0.1 0 0 1000000",
            "sleep 0.2 0 0 2000000",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Memory (mean ± σ):"))
        .stdout(predicate::str::contains("Time (mean ± σ):"))
        .stdout(predicate::str::contains("used"))
        .stdout(predicate::str::contains("less memory than"));
}

#[test]
fn metric_energy_in_debug_mode_displays_energy_header_and_used_less_energy() {
    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--metric=energy",
            "sleep 0.1",
            "sleep 0.2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Energy (mean ± σ):"))
        .stdout(predicate::str::contains("Time (mean ± σ):"))
        .stdout(predicate::str::contains("used"))
        .stdout(predicate::str::contains("as"));
}

#[test]
fn metric_user_and_system_display_headers() {
    hyperfine()
        .args(["--debug-mode", "--runs=2", "--metric=user", "sleep 0.01"])
        .assert()
        .success()
        .stdout(predicate::str::contains("User Time (mean ± σ):"));

    hyperfine()
        .args(["--debug-mode", "--runs=2", "--metric=system", "sleep 0.01"])
        .assert()
        .success()
        .stdout(predicate::str::contains("System Time (mean ± σ):"));
}

#[test]
fn metric_json_export_contains_top_level_metric_and_primary_metric() {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--metric=memory",
            &format!("--export-json={path}"),
            "sleep 0.1",
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Top-level metric is "memory"
    assert_eq!(json["metric"], "memory");

    // Standard wall-time fields remain intact
    let result = &json["results"][0];
    assert!(result["mean"].is_f64());
    assert!(result["times"].is_array());

    // primary_metric exists and has metric = "memory"
    assert_eq!(result["primary_metric"]["metric"], "memory");
    assert!(result["primary_metric"]["mean"].is_f64());
    assert!(result["primary_metric"]["samples"].is_array());
}

#[test]
fn metric_wall_json_export_has_no_primary_metric_field() {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--metric=wall",
            &format!("--export-json={path}"),
            "sleep 0.1",
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(json["metric"], "wall");
    let result = &json["results"][0];
    assert!(result.get("primary_metric").is_none());
}

#[test]
fn metric_energy_without_debug_mode_fails_if_unsupported() {
    // When run natively on an unprivileged/unsupported system (such as macOS or Windows without root RAPL),
    // '--metric energy' should fail immediately with an explanatory message.
    if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
        hyperfine()
            .args(["--metric=energy", "echo 1"])
            .assert()
            .failure()
            .code(1)
            .stderr(predicate::str::contains(
                "Energy measurement is unavailable or unprivileged on this platform. Cannot use '--metric energy'.",
            ));
    }
}

#[test]
fn metric_user_and_system_relative_speed() {
    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--metric=user",
            "sleep 0.1 0.05",
            "sleep 0.2 0.10",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("User Time (mean ± σ):"))
        .stdout(predicate::str::contains("ran"))
        .stdout(predicate::str::contains("faster than"));

    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--metric=system",
            "sleep 0.1 0 0.05",
            "sleep 0.2 0 0.10",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("System Time (mean ± σ):"))
        .stdout(predicate::str::contains("ran"))
        .stdout(predicate::str::contains("faster than"));
}

#[test]
fn metric_markdown_export_uses_primary_metric_for_relative_speed() {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--metric=memory",
            &format!("--export-markdown={path}"),
            "sleep 0.1 0 0 1000000",
            "sleep 0.2 0 0 2000000",
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(path).unwrap();
    // Memory ratio is 2000000 / 1000000 = 2.00
    assert!(
        content.contains("2.00"),
        "Expected 2.00 ratio in markdown: {content}"
    );
}

#[test]
fn metric_csv_export_uses_primary_metric_for_relative_speed() {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let path = temp_file.path().to_str().unwrap();

    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--metric=memory",
            &format!("--export-csv={path}"),
            "sleep 0.1 0 0 1000000",
            "sleep 0.2 0 0 2000000",
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.contains("relative_speed"));
    // One entry is 1 (reference) and the other is 2 (2x memory)
    assert!(
        content.contains(",2,"),
        "Expected relative_speed of 2 in CSV: {content}"
    );
}

#[test]
fn metric_memory_sorting_selects_least_memory_as_reference() {
    // Command A: sleep 0.5s, 1000 bytes memory
    // Command B: sleep 0.1s, 9000 bytes memory
    // Even though Command B has shorter wall time (0.1s vs 0.5s), Command A uses less memory (1000 vs 9000).
    // With '--metric=memory', Command A must be chosen as the reference/winner.
    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--metric=memory",
            "sleep 0.5 0 0 1000",
            "sleep 0.1 0 0 9000",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("sleep 0.5 0 0 1000 used"))
        .stdout(predicate::str::contains("less memory than"));
}
