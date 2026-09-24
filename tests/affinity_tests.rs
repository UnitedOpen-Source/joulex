//! Tests for --affinity.

mod common;
use common::hyperfine;

use predicates::prelude::*;

#[cfg(target_os = "linux")]
#[test]
fn benchmarked_process_runs_only_on_the_given_cpu() {
    hyperfine()
        .args(["--runs=1", "-N", "--affinity=0", "--show-output"])
        .arg("grep Cpus_allowed_list /proc/self/status")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"Cpus_allowed_list:\s+0\n").unwrap());
}

#[cfg(target_os = "linux")]
#[test]
fn affinity_also_applies_through_the_shell() {
    hyperfine()
        .args(["--runs=1", "--affinity=0", "--show-output"])
        .arg("grep Cpus_allowed_list /proc/self/status")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"Cpus_allowed_list:\s+0\n").unwrap());
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn rejects_cpus_that_do_not_exist() {
    hyperfine()
        .args(["--runs=1", "--affinity=100000", "echo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("CPU 100000 does not exist"));
}

#[cfg(not(any(target_os = "linux", windows)))]
#[test]
fn is_rejected_where_unsupported() {
    hyperfine()
        .args(["--runs=1", "--affinity=0", "echo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "only supported on Linux and Windows",
        ));
}

#[test]
fn rejects_malformed_cpu_lists() {
    hyperfine()
        .args(["--runs=1", "--affinity=3-1", "echo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid '--affinity'"));
}
