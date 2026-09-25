//! Tests for --show-output-on-failure.

mod common;
use common::hyperfine;

use predicates::prelude::*;

#[test]
fn conflicts_with_show_output_and_output() {
    for other in ["--show-output", "--output=pipe"] {
        hyperfine()
            .args(["--show-output-on-failure", other, "echo"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("cannot be used with"));
    }
}

#[cfg(unix)]
mod unix {
    use super::*;

    #[test]
    fn a_failing_run_shows_its_output_in_the_error() {
        hyperfine()
            .args(["--runs=2", "--show-output-on-failure"])
            .arg("echo to-stdout; echo boom >&2; exit 3")
            .assert()
            .failure()
            .stderr(predicate::str::contains("non-zero exit code 3"))
            .stderr(predicate::str::contains(
                "──── stderr (last 1 lines) ────\nboom",
            ))
            .stderr(predicate::str::contains(
                "──── stdout (last 1 lines) ────\nto-stdout",
            ))
            .stderr(predicate::str::contains("--show-output-on-failure").not());
    }

    #[test]
    fn without_the_option_the_error_suggests_it() {
        hyperfine()
            .args(["--runs=2", "exit 3"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("'--show-output-on-failure'"));
    }

    #[test]
    fn successful_runs_print_nothing_extra() {
        hyperfine()
            .args(["--runs=3", "--show-output-on-failure", "--style=basic"])
            // The marker is assembled by printf, so it isn't part of the
            // command name shown in the output
            .arg("printf 'LEAK%s\\n' _MARKER; printf 'LEAK%s\\n' _MARKER >&2")
            .assert()
            .success()
            .stdout(predicate::str::contains("LEAK_MARKER").not())
            .stderr(predicate::str::contains("LEAK_MARKER").not());
    }

    #[test]
    fn ignored_failures_are_shown_as_warnings() {
        hyperfine()
            .args([
                "--runs=5",
                "-i",
                "--show-output-on-failure",
                "--style=basic",
            ])
            .arg("[ \"$JOULEX_ITERATION\" = 2 ] && { echo only-two >&2; exit 1; }; true")
            .assert()
            .success()
            .stderr(predicate::str::contains(
                "failed in benchmark iteration 2 (ignored)",
            ))
            .stderr(predicate::str::contains("only-two"))
            .stderr(predicate::str::contains("(ignored)").count(1));
    }

    #[test]
    fn at_most_three_failed_runs_are_shown() {
        hyperfine()
            .args([
                "--runs=6",
                "-i",
                "--show-output-on-failure",
                "--style=basic",
            ])
            .arg("echo x >&2; exit 1")
            .assert()
            .success()
            .stderr(predicate::str::contains("(ignored)").count(3))
            .stderr(predicate::str::contains(
                "the output of further failed runs is not shown",
            ));
    }

    /// Both pipes are drained concurrently: a command that fills both with
    /// more than a pipe buffer must not deadlock.
    #[test]
    fn large_output_on_both_streams_does_not_deadlock() {
        hyperfine()
            .args(["--runs=2", "--show-output-on-failure"])
            .arg("head -c 5000000 /dev/zero >&2; head -c 5000000 /dev/zero; echo end >&2; exit 1")
            .timeout(std::time::Duration::from_secs(60))
            .assert()
            .failure()
            .stderr(predicate::str::contains("end"))
            .stderr(predicate::str::contains("earlier characters"));
    }

    #[test]
    fn captured_output_cannot_control_the_terminal() {
        hyperfine()
            .args(["--runs=1", "--show-output-on-failure"])
            .arg("printf '\\033]0;pwned\\007\\033[31mred\\n' >&2; exit 1")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "\\u{1b}]0;pwned\\u{7}\\u{1b}[31mred",
            ))
            .stderr(predicate::str::contains("\u{1b}]0;pwned").not());
    }
}
