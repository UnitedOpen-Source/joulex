mod common;
use common::hyperfine;
use predicates::prelude::*;

#[test]
fn command_after_double_dash_is_benchmarked() {
    hyperfine()
        .args(["--debug-mode", "--runs=2", "--", "sleep", "0.3"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Benchmark 1: sleep 0.3"))
        .stdout(predicate::str::contains("300.0 ms"));
}

#[test]
fn command_after_double_dash_conflicts_with_positional_commands() {
    hyperfine()
        .args(["--debug-mode", "sleep 0.1", "--", "sleep", "0.2"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn command_after_double_dash_conflicts_with_shell() {
    hyperfine()
        .args(["--shell", "bash", "--", "echo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn command_after_double_dash_accepts_hyphen_arguments_and_parameters() {
    hyperfine()
        .args(["--debug-mode", "--runs=1", "-L", "t", "0.1,0.2"])
        .args(["--", "sleep", "{t}"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Benchmark 1: sleep 0.1"))
        .stdout(predicate::str::contains("Benchmark 2: sleep 0.2"));
}

#[cfg(unix)]
#[test]
fn command_after_double_dash_runs_without_shell_or_resplitting() {
    hyperfine()
        .args(["--runs=1", "--show-output", "--"])
        .args(["printf", "[%s]\\n", "a b", "it's", "$HOME", "-x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[a b]\n[it's]\n[$HOME]\n[-x]\n"));
}
