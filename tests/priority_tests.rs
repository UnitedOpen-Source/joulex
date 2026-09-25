//! Tests for --priority.

mod common;
use common::hyperfine;

use predicates::prelude::*;

#[test]
fn invalid_priority_is_rejected() {
    hyperfine()
        .args(["--priority=fast", "echo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value 'fast'"));
}

#[test]
fn normal_priority_is_accepted() {
    hyperfine()
        .args(["--debug-mode", "--runs=1", "--priority=normal", "sleep 0.1"])
        .assert()
        .success();
}

#[cfg(unix)]
mod unix {
    use super::*;

    /// Print the scheduling policy (Linux) or the nice value (other Unix) of
    /// the benchmarked process.
    fn probe() -> &'static str {
        if cfg!(target_os = "linux") {
            // Field 41 of /proc/<pid>/stat is the scheduling policy
            "echo policy=$(cut -d' ' -f41 /proc/$$/stat)"
        } else {
            "echo nice=$(ps -o nice= -p $$ | tr -d ' ')"
        }
    }

    #[test]
    fn idle_lowers_the_priority_of_the_benchmarked_process() {
        let expected = if cfg!(target_os = "linux") {
            "policy=5" // SCHED_IDLE
        } else {
            "nice=19"
        };
        hyperfine()
            .args(["--runs=1", "--show-output", "--priority=idle", probe()])
            .assert()
            .success()
            .stdout(predicate::str::contains(expected));
    }

    #[test]
    fn default_priority_is_unchanged() {
        let expected = if cfg!(target_os = "linux") {
            "policy=0" // SCHED_OTHER
        } else {
            "nice=0"
        };
        hyperfine()
            .args(["--runs=1", "--show-output", probe()])
            .assert()
            .success()
            .stdout(predicate::str::contains(expected));
    }

    #[test]
    fn high_needs_privileges_and_explains_how_to_get_them() {
        // Whether raising the priority is allowed depends on the environment
        // (root, CAP_SYS_NICE; root in a container usually lacks the latter),
        // so accept both outcomes, but a failure must explain itself.
        let output = hyperfine()
            .args(["--runs=1", "-N", "--priority=high", "true"])
            .output()
            .unwrap();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("could not set '--priority high'"),
                "{stderr}"
            );
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn realtime_is_rejected_where_unsupported() {
        hyperfine()
            .args(["--priority=realtime", "true"])
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "only supported on Linux and Windows",
            ));
    }
}
