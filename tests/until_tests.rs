//! Tests for --until and --until-stderr (#79).

mod common;
use common::hyperfine;

use predicates::prelude::*;

#[test]
fn conflicts_with_show_output_and_output() {
    hyperfine()
        .args(["--until", "READY", "--show-output", "echo 1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));

    hyperfine()
        .args(["--until", "READY", "--show-output-on-failure", "echo 1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));

    hyperfine()
        .args(["--until", "READY", "--output", "pipe", "echo 1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn empty_until_pattern_rejected() {
    hyperfine()
        .args(["--until", "", "echo 1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The --until pattern cannot be empty",
        ));
}

#[test]
fn until_stderr_without_until_rejected() {
    hyperfine()
        .args(["--until-stderr", "echo 1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "required arguments were not provided",
        ));
}

#[cfg(unix)]
mod unix {
    use super::*;

    #[test]
    fn fast_readiness_stops_timer_and_kills_process() {
        let start = std::time::Instant::now();
        hyperfine()
            .args([
                "--until",
                "READY",
                "-N",
                "-r",
                "2",
                "sh -c 'sleep 0.1; echo READY; sleep 30'",
            ])
            .assert()
            .success();

        assert!(
            start.elapsed() < std::time::Duration::from_secs(3),
            "Process did not terminate after readiness match: {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn until_stderr_matches_stderr() {
        let start = std::time::Instant::now();
        hyperfine()
            .args([
                "--until",
                "READY_ERR",
                "--until-stderr",
                "-N",
                "-r",
                "2",
                "sh -c 'sleep 0.1; echo READY_ERR >&2; sleep 30'",
            ])
            .assert()
            .success();

        assert!(
            start.elapsed() < std::time::Duration::from_secs(3),
            "Process did not terminate after stderr readiness match: {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn exiting_without_pattern_fails() {
        hyperfine()
            .args([
                "--until",
                "EXPECTED_READY",
                "-N",
                "-r",
                "2",
                "sh -c 'echo SOMETHING_ELSE; exit 0'",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "Command exited without matching '--until' pattern",
            ));
    }

    #[test]
    fn ignoring_failure_with_until() {
        hyperfine()
            .args([
                "--until",
                "EXPECTED_READY",
                "-i",
                "-N",
                "-r",
                "2",
                "sh -c 'echo SOMETHING_ELSE; exit 0'",
            ])
            .assert()
            .success();
    }

    #[test]
    fn alias_ready_when_works() {
        let start = std::time::Instant::now();
        hyperfine()
            .args([
                "--ready-when",
                "SERVER_LISTENING",
                "-N",
                "-r",
                "1",
                "sh -c 'sleep 0.05; echo SERVER_LISTENING; sleep 30'",
            ])
            .assert()
            .success();

        assert!(
            start.elapsed() < std::time::Duration::from_secs(3),
            "Alias --ready-when did not terminate early: {:?}",
            start.elapsed()
        );
    }
}
