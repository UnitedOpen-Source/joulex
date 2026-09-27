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

    #[test]
    fn until_kills_orphan_descendant_ignoring_sigterm() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("child.pid");
        let ready_file = dir.path().join("child.ready");
        let survived_file = dir.path().join("child.survived");
        let script_file = dir.path().join("test_orphan.sh");

        let mut f = std::fs::File::create(&script_file).unwrap();
        writeln!(
            f,
            "#!/bin/sh\n(trap '' TERM; echo ready > \"$2\"; sleep 1; echo survived > \"$3\"; sleep 30) &\necho $! > \"$1\"\nwhile [ ! -f \"$2\" ]; do sleep 0.01; done\necho READY\nexit 0"
        )
        .unwrap();
        drop(f);

        let script_cmd = format!(
            "sh {} {} {} {}",
            script_file.to_str().unwrap(),
            pid_file.to_str().unwrap(),
            ready_file.to_str().unwrap(),
            survived_file.to_str().unwrap()
        );

        let start = std::time::Instant::now();
        hyperfine()
            .args(["--until", "READY", "-N", "-r", "1", &script_cmd])
            .assert()
            .success();

        assert!(
            start.elapsed() < std::time::Duration::from_secs(3),
            "Benchmark took too long: {:?}",
            start.elapsed()
        );

        let child_pid_str =
            std::fs::read_to_string(&pid_file).expect("child pid file should have been written");
        let child_pid: libc::pid_t = child_pid_str.trim().parse().expect("valid child pid");

        std::thread::sleep(std::time::Duration::from_millis(1500));

        let res = unsafe { libc::kill(child_pid, 0) };
        let is_alive =
            res == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH);
        if is_alive {
            unsafe {
                libc::kill(child_pid, libc::SIGKILL);
            }
        }
        assert!(
            !survived_file.exists(),
            "Descendant process {child_pid} continued running after --until returned"
        );
    }
}
