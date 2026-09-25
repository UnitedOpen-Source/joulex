//! `--debug-mode` must reject commands it can't simulate instead of panicking.

mod common;
use common::hyperfine;

use predicates::prelude::*;

#[test]
fn debug_mode_rejects_commands_other_than_sleep() {
    hyperfine()
        .args(["--debug-mode", "--runs=2", "echo hi"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "'--debug-mode' only simulates commands of the form 'sleep <seconds>', got 'echo hi'",
        ))
        .stderr(predicate::str::contains("panicked").not());
}

#[test]
fn debug_mode_rejects_an_invalid_simulated_shell() {
    hyperfine()
        .args(["--debug-mode", "--runs=2", "--shell=bash", "sleep 0.1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("got 'bash'"))
        .stderr(predicate::str::contains("panicked").not());
}

#[test]
fn debug_mode_accepts_a_simulated_shell() {
    hyperfine()
        .args([
            "--debug-mode",
            "--runs=2",
            "--shell=sleep 0.01",
            "sleep 0.1",
        ])
        .assert()
        .success();
}
