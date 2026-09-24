mod common;
use common::hyperfine;

use predicates::prelude::*;

/// Platform-specific I/O utility.
/// - On Unix-like systems, defaults to `cat`.
/// - On Windows, uses `findstr` as an alternative.
///   See: <https://superuser.com/questions/853580/real-windows-equivalent-to-cat-stdin>
const STDIN_READ_COMMAND: &str = if cfg!(windows) { "findstr x*" } else { "cat" };

pub fn hyperfine_debug() -> assert_cmd::Command {
    let mut cmd = hyperfine();
    cmd.arg("--debug-mode");
    cmd
}

#[test]
fn runs_successfully() {
    hyperfine()
        .arg("--runs=2")
        .arg("echo dummy benchmark")
        .assert()
        .success();
}

#[test]
fn one_run_is_supported() {
    hyperfine()
        .arg("--runs=1")
        .arg("echo dummy benchmark")
        .assert()
        .success();
}

#[test]
fn can_run_commands_without_a_shell() {
    hyperfine()
        .arg("--runs=1")
        .arg("--show-output")
        .arg("--shell=none")
        .arg("echo 'hello world' argument2")
        .assert()
        .success()
        .stdout(predicate::str::contains("hello world argument2"));
}

#[test]
fn fails_with_wrong_number_of_command_name_arguments() {
    hyperfine()
        .arg("--command-name=a")
        .arg("--command-name=b")
        .arg("echo a")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Too many --command-name options"));
}

#[test]
fn fails_with_wrong_number_of_prepare_options() {
    hyperfine()
        .arg("--runs=1")
        .arg("--prepare=echo a")
        .arg("--prepare=echo b")
        .arg("echo a")
        .arg("echo b")
        .assert()
        .success();

    hyperfine()
        .arg("--runs=1")
        .arg("--prepare=echo ref")
        .arg("--prepare=echo a")
        .arg("--prepare=echo b")
        .arg("--reference=echo ref")
        .arg("echo a")
        .arg("echo b")
        .assert()
        .success();

    hyperfine()
        .arg("--runs=1")
        .arg("--prepare=echo a")
        .arg("--prepare=echo b")
        .arg("echo a")
        .arg("echo b")
        .arg("echo c")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The '--prepare' option has to be provided",
        ));

    hyperfine()
        .arg("--runs=1")
        .arg("--prepare=echo a")
        .arg("--prepare=echo b")
        .arg("--reference=echo ref")
        .arg("echo a")
        .arg("echo b")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The '--prepare' option has to be provided",
        ));
}

#[test]
fn fails_with_wrong_number_of_conclude_options() {
    hyperfine()
        .arg("--runs=1")
        .arg("--conclude=echo a")
        .arg("--conclude=echo b")
        .arg("echo a")
        .arg("echo b")
        .assert()
        .success();

    hyperfine()
        .arg("--runs=1")
        .arg("--conclude=echo ref")
        .arg("--conclude=echo a")
        .arg("--conclude=echo b")
        .arg("--reference=echo ref")
        .arg("echo a")
        .arg("echo b")
        .assert()
        .success();

    hyperfine()
        .arg("--runs=1")
        .arg("--conclude=echo a")
        .arg("--conclude=echo b")
        .arg("echo a")
        .arg("echo b")
        .arg("echo c")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The '--conclude' option has to be provided",
        ));

    hyperfine()
        .arg("--runs=1")
        .arg("--conclude=echo a")
        .arg("--conclude=echo b")
        .arg("--reference=echo ref")
        .arg("echo a")
        .arg("echo b")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The '--conclude' option has to be provided",
        ));
}

#[test]
fn fails_with_duplicate_parameter_names() {
    hyperfine()
        .arg("--parameter-list")
        .arg("x")
        .arg("1,2,3")
        .arg("--parameter-list")
        .arg("x")
        .arg("a,b,c")
        .arg("echo test")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Duplicate parameter names: x"));
}

#[test]
fn fails_for_unknown_command() {
    hyperfine()
        .arg("--runs=1")
        .arg("some-nonexisting-program-b5d9574198b7e4b12a71fa4747c0a577")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Command terminated with non-zero exit code",
        ));
}

#[test]
fn fails_for_unknown_command_without_shell() {
    hyperfine()
        .arg("--shell=none")
        .arg("--runs=1")
        .arg("some-nonexisting-program-b5d9574198b7e4b12a71fa4747c0a577")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Failed to run command 'some-nonexisting-program-b5d9574198b7e4b12a71fa4747c0a577'",
        ));
}

#[cfg(unix)]
#[test]
fn fails_for_failing_command_without_shell() {
    hyperfine()
        .arg("--shell=none")
        .arg("--runs=1")
        .arg("false")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Command terminated with non-zero exit code",
        ));
}

#[test]
fn fails_for_unknown_setup_command() {
    hyperfine()
        .arg("--runs=1")
        .arg("--setup=some-nonexisting-program-b5d9574198b7e4b12a71fa4747c0a577")
        .arg("echo test")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The setup command terminated with a non-zero exit code.",
        ));
}

#[test]
fn fails_for_unknown_cleanup_command() {
    hyperfine()
        .arg("--runs=1")
        .arg("--cleanup=some-nonexisting-program-b5d9574198b7e4b12a71fa4747c0a577")
        .arg("echo test")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The cleanup command terminated with a non-zero exit code.",
        ));
}

#[test]
fn fails_for_unknown_prepare_command() {
    hyperfine()
        .arg("--prepare=some-nonexisting-program-b5d9574198b7e4b12a71fa4747c0a577")
        .arg("echo test")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The preparation command terminated with a non-zero exit code.",
        ));
}

#[test]
fn fails_for_unknown_conclude_command() {
    hyperfine()
        .arg("--conclude=some-nonexisting-program-b5d9574198b7e4b12a71fa4747c0a577")
        .arg("echo test")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The conclusion command terminated with a non-zero exit code.",
        ));
}

#[cfg(unix)]
#[test]
fn can_run_failing_commands_with_ignore_failure_option() {
    hyperfine()
        .arg("false")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Command terminated with non-zero exit code",
        ));

    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-failure")
        .arg("false")
        .assert()
        .success();
}

#[cfg(unix)]
#[test]
fn can_ignore_specific_exit_codes() {
    // Test that specifying exit code 1 ignores it
    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-failure=1")
        .arg("exit 1")
        .assert()
        .success();

    // Test that other exit codes still fail
    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-failure=1")
        .arg("exit 2")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Command terminated with non-zero exit code 2",
        ));
}

#[cfg(unix)]
#[test]
fn can_ignore_multiple_exit_codes() {
    // Test that all specified exit codes are ignored
    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-failure=1,2,3")
        .arg("exit 1")
        .assert()
        .success();

    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-failure=1,2,3")
        .arg("exit 2")
        .assert()
        .success();

    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-failure=1,2,3")
        .arg("exit 3")
        .assert()
        .success();

    // Test that other exit codes still fail
    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-failure=1,2,3")
        .arg("exit 4")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Command terminated with non-zero exit code 4",
        ));
}

#[cfg(unix)]
#[test]
fn ignore_exit_code_is_an_alias_for_ignore_failure() {
    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-exit-code")
        .arg("false")
        .assert()
        .success();

    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-exit-code=1")
        .arg("exit 1")
        .assert()
        .success();

    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-exit-code=1")
        .arg("exit 2")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Use the '-i'/'--ignore-exit-code' option",
        ));
}

#[cfg(unix)]
#[test]
fn ignore_failure_with_all_non_zero() {
    // Test explicit "all-non-zero" mode
    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-failure=all-non-zero")
        .arg("exit 5")
        .assert()
        .success();
}

#[test]
fn shows_output_of_benchmarked_command() {
    hyperfine()
        .arg("--runs=2")
        .arg("--command-name=dummy")
        .arg("--show-output")
        .arg("echo 4fd47015")
        .assert()
        .success()
        .stdout(predicate::str::contains("4fd47015").count(2));
}

#[test]
fn runs_commands_using_user_defined_shell() {
    hyperfine()
        .arg("--runs=1")
        .arg("--show-output")
        .arg("--shell")
        .arg("echo 'custom_shell' '--shell-arg'")
        .arg("echo benchmark")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("custom_shell --shell-arg -c echo benchmark").or(
                predicate::str::contains("custom_shell --shell-arg /C echo benchmark"),
            ),
        );
}

#[test]
fn can_pass_input_to_command_from_a_file() {
    hyperfine()
        .arg("--runs=1")
        .arg("--input=example_input_file.txt")
        .arg("--show-output")
        .arg(STDIN_READ_COMMAND)
        .assert()
        .success()
        .stdout(predicate::str::contains("This text is part of a file"));
}

#[test]
fn fails_if_invalid_stdin_data_file_provided() {
    hyperfine()
        .arg("--runs=1")
        .arg("--input=example_non_existent_file.txt")
        .arg("--show-output")
        .arg(STDIN_READ_COMMAND)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The file 'example_non_existent_file.txt' specified as '--input' does not exist",
        ));
}

#[test]
fn returns_mean_time_in_correct_unit() {
    hyperfine_debug()
        .arg("sleep 1.234")
        .assert()
        .success()
        .stdout(predicate::str::contains("Time (mean ± σ):      1.234 s ±"));

    hyperfine_debug()
        .arg("sleep 0.123")
        .assert()
        .success()
        .stdout(predicate::str::contains("Time (mean ± σ):     123.0 ms ±"));

    hyperfine_debug()
        .arg("--time-unit=millisecond")
        .arg("sleep 1.234")
        .assert()
        .success()
        .stdout(predicate::str::contains("Time (mean ± σ):     1234.0 ms ±"));

    hyperfine_debug()
        .arg("--time-unit=microsecond")
        .arg("sleep 1.234")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Time (mean ± σ):     1234000.0 µs ±",
        ));
}

#[test]
fn performs_ten_runs_for_slow_commands() {
    hyperfine_debug()
        .arg("sleep 0.5")
        .assert()
        .success()
        .stdout(predicate::str::contains("10 runs"));
}

#[test]
fn performs_three_seconds_of_benchmarking_for_fast_commands() {
    hyperfine_debug()
        .arg("sleep 0.01")
        .assert()
        .success()
        .stdout(predicate::str::contains("300 runs"));
}

#[test]
fn takes_shell_spawning_time_into_account_for_computing_number_of_runs() {
    hyperfine_debug()
        .arg("--shell=sleep 0.02")
        .arg("sleep 0.01")
        .assert()
        .success()
        .stdout(predicate::str::contains("100 runs"));
}

#[test]
fn takes_preparation_command_into_account_for_computing_number_of_runs() {
    hyperfine_debug()
        .arg("--prepare=sleep 0.02")
        .arg("sleep 0.01")
        .assert()
        .success()
        .stdout(predicate::str::contains("100 runs"));

    // Shell overhead needs to be added to both the prepare command and the actual command,
    // leading to a total benchmark time of (prepare + shell + cmd + shell = 0.1 s)
    hyperfine_debug()
        .arg("--shell=sleep 0.01")
        .arg("--prepare=sleep 0.03")
        .arg("sleep 0.05")
        .assert()
        .success()
        .stdout(predicate::str::contains("30 runs"));
}

#[test]
fn takes_conclusion_command_into_account_for_computing_number_of_runs() {
    hyperfine_debug()
        .arg("--conclude=sleep 0.02")
        .arg("sleep 0.01")
        .assert()
        .success()
        .stdout(predicate::str::contains("100 runs"));

    // Shell overhead needs to be added to both the conclude command and the actual command,
    // leading to a total benchmark time of (cmd + shell + conclude + shell = 0.1 s)
    hyperfine_debug()
        .arg("--shell=sleep 0.01")
        .arg("--conclude=sleep 0.03")
        .arg("sleep 0.05")
        .assert()
        .success()
        .stdout(predicate::str::contains("30 runs"));
}

#[test]
fn takes_both_preparation_and_conclusion_command_into_account_for_computing_number_of_runs() {
    hyperfine_debug()
        .arg("--prepare=sleep 0.01")
        .arg("--conclude=sleep 0.01")
        .arg("sleep 0.01")
        .assert()
        .success()
        .stdout(predicate::str::contains("100 runs"));

    // Shell overhead needs to be added to both the prepare, conclude and the actual command,
    // leading to a total benchmark time of (prepare + shell + cmd + shell + conclude + shell = 0.1 s)
    hyperfine_debug()
        .arg("--shell=sleep 0.01")
        .arg("--prepare=sleep 0.01")
        .arg("--conclude=sleep 0.01")
        .arg("sleep 0.05")
        .assert()
        .success()
        .stdout(predicate::str::contains("30 runs"));
}

#[test]
fn shows_benchmark_comparison_with_relative_times() {
    hyperfine_debug()
        .arg("sleep 1.0")
        .arg("sleep 2.0")
        .arg("sleep 3.0")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("2.00 ± 0.00 times faster")
                .and(predicate::str::contains("3.00 ± 0.00 times faster")),
        );
}

#[test]
fn shows_benchmark_comparison_with_same_time() {
    hyperfine_debug()
        .arg("--command-name=A")
        .arg("--command-name=B")
        .arg("sleep 1.0")
        .arg("sleep 1.0")
        .arg("sleep 2.0")
        .arg("sleep 1000.0")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("As fast (1.00 ± 0.00) as")
                .and(predicate::str::contains("2.00 ± 0.00 times faster"))
                .and(predicate::str::contains("1000.00 ± 0.00 times faster")),
        );
}

#[test]
fn shows_benchmark_comparison_relative_to_reference() {
    hyperfine_debug()
        .arg("--reference=sleep 2.0")
        .arg("sleep 1.0")
        .arg("sleep 3.0")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("2.00 ± 0.00 times slower")
                .and(predicate::str::contains("1.50 ± 0.00 times faster")),
        );
}

#[test]
fn shows_reference_name() {
    hyperfine_debug()
        .arg("--reference=sleep 2.0")
        .arg("--reference-name=refabc123")
        .arg("sleep 1.0")
        .arg("sleep 3.0")
        .assert()
        .success()
        .stdout(predicate::str::contains("Benchmark 1: refabc123"));
}

#[test]
fn shows_faster_slower_annotations_with_sort_command_and_reference() {
    hyperfine_debug()
        .arg("--sort=command")
        .arg("--reference=sleep 2.0")
        .arg("sleep 1.0")
        .arg("sleep 3.0")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Relative speed comparison (reference: sleep 2.0)")
                .and(predicate::str::contains("times faster than sleep 2.0"))
                .and(predicate::str::contains("times slower than sleep 2.0")),
        );
}

#[test]
fn shows_faster_slower_natural_layout_with_sort_command_and_reference() {
    let assert = hyperfine_debug()
        .arg("--sort=command")
        .arg("--reference=sleep 2.0")
        .arg("sleep 1.0")
        .arg("sleep 3.0")
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("Relative speed comparison (reference: sleep 2.0)"));
    assert!(stdout.contains("sleep 2.0  (reference)"));

    let mut found_faster = false;
    let mut found_slower = false;
    for line in stdout.lines() {
        if line.contains("times faster than sleep 2.0") {
            found_faster = true;
            assert!(line.contains("sleep 1.0"));
            let pos_cmd = line.find("sleep 1.0").unwrap();
            let pos_ann = line.find("times faster than").unwrap();
            assert!(
                pos_cmd < pos_ann,
                "command should precede annotation in: {line}"
            );
        }
        if line.contains("times slower than sleep 2.0") {
            found_slower = true;
            assert!(line.contains("sleep 3.0"));
            let pos_cmd = line.find("sleep 3.0").unwrap();
            let pos_ann = line.find("times slower than").unwrap();
            assert!(
                pos_cmd < pos_ann,
                "command should precede annotation in: {line}"
            );
        }
    }
    assert!(found_faster && found_slower);
}

#[test]
fn performs_all_benchmarks_in_parameter_scan() {
    hyperfine_debug()
        .arg("--parameter-scan")
        .arg("time")
        .arg("30")
        .arg("45")
        .arg("--parameter-step-size")
        .arg("5")
        .arg("sleep {time}")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Benchmark 1: sleep 30")
                .and(predicate::str::contains("Benchmark 2: sleep 35"))
                .and(predicate::str::contains("Benchmark 3: sleep 40"))
                .and(predicate::str::contains("Benchmark 4: sleep 45"))
                .and(predicate::str::contains("Benchmark 5: sleep 50").not()),
        );
}

#[test]
fn performs_reference_and_all_benchmarks_in_parameter_scan() {
    hyperfine_debug()
        .arg("--reference=sleep 25")
        .arg("--parameter-scan")
        .arg("time")
        .arg("30")
        .arg("45")
        .arg("--parameter-step-size")
        .arg("5")
        .arg("sleep {time}")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Benchmark 1: sleep 25")
                .and(predicate::str::contains("Benchmark 2: sleep 30"))
                .and(predicate::str::contains("Benchmark 3: sleep 35"))
                .and(predicate::str::contains("Benchmark 4: sleep 40"))
                .and(predicate::str::contains("Benchmark 5: sleep 45"))
                .and(predicate::str::contains("Benchmark 6: sleep 50").not()),
        );
}

#[test]
fn intermediate_results_are_not_exported_to_stdout() {
    hyperfine_debug()
        .arg("--style=none") // To only see the Markdown export on stdout
        .arg("--export-markdown")
        .arg("-")
        .arg("sleep 1")
        .arg("sleep 2")
        .assert()
        .success()
        .stdout(
            (predicate::str::contains("sleep 1").count(1))
                .and(predicate::str::contains("sleep 2").count(1)),
        );
}

#[test]
#[cfg(unix)]
fn exports_intermediate_results_to_file() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("results.md");

    hyperfine()
        .arg("--runs=1")
        .arg("--export-markdown")
        .arg(&export_path)
        .arg("true")
        .arg("false")
        .assert()
        .failure();

    let contents = std::fs::read_to_string(export_path).unwrap();
    assert!(contents.contains("true"));
}

#[test]
fn unused_parameters_are_shown_in_benchmark_name() {
    hyperfine()
        .arg("--runs=2")
        .arg("--parameter-list")
        .arg("branch")
        .arg("master,feature")
        .arg("echo test")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("echo test (branch = master)")
                .and(predicate::str::contains("echo test (branch = feature)")),
        );
}

#[test]
fn speed_comparison_sort_order() {
    for sort_order in ["auto", "mean-time"] {
        hyperfine_debug()
            .arg("sleep 2")
            .arg("sleep 1")
            .arg(format!("--sort={sort_order}"))
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "sleep 1 ran\n    2.00 ± 0.00 times faster than sleep 2",
            ));
    }

    hyperfine_debug()
        .arg("sleep 2")
        .arg("sleep 1")
        .arg("--sort=command")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "2.00 ±  0.00  sleep 2\n        1.00          sleep 1",
        ));
}

#[cfg(windows)]
#[test]
fn windows_quote_args() {
    hyperfine()
        .arg("more \"example_input_file.txt\"")
        .assert()
        .success();
}

#[cfg(windows)]
#[test]
fn windows_quote_before_quote_args() {
    hyperfine()
        .arg("dir \"..\\src\\\" \"..\\tests\\\"")
        .assert()
        .success();
}

#[test]
fn fails_with_zero_runs() {
    hyperfine()
        .arg("--runs=0")
        .arg("echo a")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "At least one run has to be performed. Please specify a value larger than zero for '--runs'",
        ));

    hyperfine()
        .arg("--max-runs=0")
        .arg("echo a")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "At least one run has to be performed. Please specify a value larger than zero for '--max-runs'",
        ));
}

#[test]
fn can_generate_shell_completions() {
    hyperfine()
        .arg("--generate-completions=bash")
        .assert()
        .success()
        .stdout(predicate::str::contains("complete -F _joulex"));

    hyperfine()
        .arg("--generate-completions=zsh")
        .assert()
        .success()
        .stdout(predicate::str::contains("compdef _joulex joulex"));
}

#[test]
fn can_export_unified() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("results.csv");

    hyperfine()
        .arg("--runs=1")
        .arg("--export")
        .arg(&export_path)
        .arg("echo test")
        .assert()
        .success();

    let contents = std::fs::read_to_string(export_path).unwrap();
    assert!(contents.contains("command,mean,stddev,median,user,system,min,max"));
    assert!(contents.contains("echo test"));
}

#[test]
#[cfg(unix)]
fn filter_failed_commands() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("results.json");

    hyperfine()
        .arg("--runs=1")
        .arg("--ignore-failure")
        .arg("--filter-failed")
        .arg("--export-json")
        .arg(&export_path)
        .arg("echo success")
        .arg("false")
        .assert()
        .success();

    let contents = std::fs::read_to_string(export_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&contents).unwrap();
    // Only the results are filtered; the command line in the metadata still
    // lists every command.
    let commands: Vec<&str> = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["command"].as_str().unwrap())
        .collect();
    assert_eq!(commands, vec!["echo success"]);
}

#[test]
#[cfg(unix)]
fn iteration_env_var_forwarded() {
    hyperfine()
        .arg("--runs=1")
        .arg("--prepare=echo prep-$JOULEX_ITERATION")
        .arg("--show-output")
        .arg("echo run-$JOULEX_ITERATION")
        .assert()
        .success()
        .stdout(predicate::str::contains("prep-0"))
        .stdout(predicate::str::contains("run-0"));
}

#[test]
fn runs_with_parameter_file() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, "foo\nbar").unwrap();

    hyperfine()
        .arg("--runs=1")
        .arg("--parameter-file")
        .arg("testvar")
        .arg(temp.path())
        .arg("--show-output")
        .arg("echo val-{testvar}")
        .assert()
        .success()
        .stdout(predicate::str::contains("val-foo"))
        .stdout(predicate::str::contains("val-bar"));
}

#[test]
fn fails_with_missing_parameter_file() {
    hyperfine()
        .arg("--runs=1")
        .arg("--parameter-file")
        .arg("testvar")
        .arg("non_existent_file_12345.txt")
        .arg("echo {testvar}")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Could not read parameter file"));
}

#[test]
fn can_import_json_and_compare_with_live_command() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("baseline.json");

    hyperfine()
        .arg("--runs=1")
        .arg("--shell=none")
        .arg("--export-json")
        .arg(&export_path)
        .arg("sleep 0.01")
        .assert()
        .success();

    hyperfine()
        .arg("--runs=1")
        .arg("--shell=none")
        .arg("--import-json")
        .arg(&export_path)
        .arg("sleep 0.02")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Benchmark 1: sleep 0.01 (imported)",
        ))
        .stdout(predicate::str::contains("Benchmark 2: sleep 0.02"))
        .stdout(predicate::str::contains("Summary"));
}

#[test]
fn can_convert_imported_json_without_live_command() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_json_path = tempdir.path().join("baseline.json");
    let export_md_path = tempdir.path().join("converted.md");

    hyperfine()
        .arg("--runs=1")
        .arg("--export-json")
        .arg(&export_json_path)
        .arg("echo baseline_test")
        .assert()
        .success();

    hyperfine()
        .arg("--import-json")
        .arg(&export_json_path)
        .arg("--export-markdown")
        .arg(&export_md_path)
        .assert()
        .success();

    let contents = std::fs::read_to_string(export_md_path).unwrap();
    assert!(contents.contains("echo baseline_test"));
}

#[test]
fn fails_with_missing_import_json_file() {
    hyperfine()
        .arg("--import-json")
        .arg("non_existent_import_file.json")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Could not open import file 'non_existent_import_file.json'",
        ));
}

#[test]
fn exports_detailed_user_and_system_times_in_json() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("results.json");

    hyperfine()
        .arg("--runs=2")
        .arg("--export-json")
        .arg(&export_path)
        .arg("echo test_times")
        .assert()
        .success();

    let contents = std::fs::read_to_string(export_path).unwrap();
    assert!(contents.contains("\"user_times\": ["));
    assert!(contents.contains("\"system_times\": ["));
}

#[test]
fn suppresses_outlier_warnings_flag() {
    hyperfine()
        .arg("--runs=2")
        .arg("--suppress-outlier-warnings")
        .arg("echo test_suppress")
        .assert()
        .success()
        .stderr(predicate::str::contains("Statistical outliers").not())
        .stderr(predicate::str::contains("initial run was unusually slow").not());
}

#[test]
fn export_csv_with_reference_and_parameter_scan() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("results.csv");

    hyperfine()
        .arg("--runs=1")
        .arg("--shell=none")
        .arg("--reference=sleep 0.01")
        .arg("--parameter-list")
        .arg("secs")
        .arg("0.01,0.02")
        .arg("--export-csv")
        .arg(&export_path)
        .arg("sleep {secs}")
        .assert()
        .success();

    let contents = std::fs::read_to_string(export_path).unwrap();
    assert!(contents.contains("command,mean,stddev,median,user,system,min,max,parameter_secs"));
    assert!(contents.contains("sleep 0.01"));
}

#[test]
fn off_cpu_warning_and_cpu_percent_display() {
    hyperfine()
        .arg("--runs=2")
        .arg("--shell=none")
        .arg("sleep 0.12")
        .assert()
        .success()
        .stdout(predicate::str::contains("CPU:"))
        .stderr(predicate::str::contains(
            "Substantial off-CPU time detected",
        ));
}

#[test]
fn off_cpu_warning_suppressed_by_flag() {
    hyperfine()
        .arg("--runs=2")
        .arg("--shell=none")
        .arg("--no-off-cpu-warning")
        .arg("sleep 0.12")
        .assert()
        .success()
        .stdout(predicate::str::contains("CPU:"))
        .stderr(predicate::str::contains("Substantial off-CPU time detected").not());
}

#[test]
fn off_cpu_warning_not_suppressed_by_outlier_flag() {
    hyperfine()
        .arg("--runs=2")
        .arg("--shell=none")
        .arg("--suppress-outlier-warnings")
        .arg("sleep 0.12")
        .assert()
        .success()
        .stdout(predicate::str::contains("CPU:"))
        .stderr(predicate::str::contains(
            "Substantial off-CPU time detected",
        ));
}

#[test]
fn off_cpu_warning_suppressed_by_suppress_warnings_flag() {
    hyperfine()
        .arg("--runs=2")
        .arg("--shell=none")
        .arg("--suppress-warnings=off-cpu")
        .arg("sleep 0.12")
        .assert()
        .success()
        .stdout(predicate::str::contains("CPU:"))
        .stderr(predicate::str::contains("Substantial off-CPU time detected").not());

    hyperfine()
        .arg("--runs=2")
        .arg("--shell=none")
        .arg("--suppress-warnings=all")
        .arg("sleep 0.12")
        .assert()
        .success()
        .stdout(predicate::str::contains("CPU:"))
        .stderr(predicate::str::contains("Substantial off-CPU time detected").not());

    hyperfine()
        .arg("--runs=2")
        .arg("--shell=none")
        .arg("--suppress-warnings=outliers")
        .arg("sleep 0.12")
        .assert()
        .success()
        .stdout(predicate::str::contains("CPU:"))
        .stderr(predicate::str::contains(
            "Substantial off-CPU time detected",
        ));
}

#[test]
fn json_export_includes_cpu_percent() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("results.json");

    hyperfine()
        .arg("--runs=1")
        .arg("--export-json")
        .arg(&export_path)
        .arg("echo test_cpu_json")
        .assert()
        .success();

    let contents = std::fs::read_to_string(export_path).unwrap();
    assert!(contents.contains("\"cpu_percent\":"));
}

#[test]
fn multiple_parameter_scans_csv_export() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("results.csv");

    hyperfine()
        .arg("--runs=1")
        .arg("--export-csv")
        .arg(&export_path)
        .arg("-P")
        .arg("a")
        .arg("1")
        .arg("2")
        .arg("-P")
        .arg("b")
        .arg("10")
        .arg("11")
        .arg("echo {a} {b}")
        .assert()
        .success();

    let contents = std::fs::read_to_string(export_path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 5); // 1 header line + 4 data rows
    assert!(lines[0].contains("parameter_a"));
    assert!(lines[0].contains("parameter_b"));
}

#[test]
fn multiple_parameter_scans_and_list_combined_json_export() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("results.json");

    hyperfine()
        .arg("--runs=1")
        .arg("--export-json")
        .arg(&export_path)
        .arg("-P")
        .arg("a")
        .arg("1")
        .arg("2")
        .arg("-L")
        .arg("opt")
        .arg("x,y")
        .arg("echo {a} {opt}")
        .assert()
        .success();

    let contents = std::fs::read_to_string(export_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
    let results = parsed["results"].as_array().unwrap();
    assert_eq!(results.len(), 4);
    for entry in results {
        let params = entry["parameters"].as_object().unwrap();
        assert!(params.contains_key("a"));
        assert!(params.contains_key("opt"));
    }
}

#[test]
fn multiple_parameter_scans_with_step_size_cli_error() {
    hyperfine()
        .arg("-P")
        .arg("a")
        .arg("1")
        .arg("5")
        .arg("-P")
        .arg("b")
        .arg("1")
        .arg("5")
        .arg("-D")
        .arg("2")
        .arg("echo {a} {b}")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The '--parameter-step-size' ('-D') option cannot be used when multiple '--parameter-scan' ('-P') options are specified",
        ));
}

#[test]
fn omit_failed_runs_requires_ignore_failure() {
    hyperfine()
        .arg("--omit-failed-runs")
        .arg("echo a")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--ignore-failure"));
}

#[cfg(unix)]
#[test]
fn omit_failed_runs_excludes_failures_from_statistics() {
    hyperfine()
        .arg("--ignore-failure")
        .arg("--omit-failed-runs")
        .arg("--runs=5")
        .arg("sh -c 'if [ \"$JOULEX_ITERATION\" = \"2\" ] || [ \"$JOULEX_ITERATION\" = \"4\" ]; then exit 1; else sleep 0.01; fi'")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 failed runs omitted"))
        .stderr(predicate::str::contains(
            "Omitted 2 of 5 benchmark runs with non-zero exit codes",
        ));
}

#[cfg(unix)]
#[test]
fn omit_failed_runs_all_failed_errors() {
    hyperfine()
        .arg("--ignore-failure")
        .arg("--omit-failed-runs")
        .arg("--runs=3")
        .arg("false")
        .assert()
        .failure()
        .stderr(predicate::str::contains("All benchmark runs failed"));
}

#[cfg(unix)]
#[test]
fn omit_failed_runs_json_export() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("results.json");

    hyperfine()
        .arg("--ignore-failure")
        .arg("--omit-failed-runs")
        .arg("--runs=5")
        .arg("--export-json")
        .arg(&export_path)
        .arg("sh -c 'if [ \"$JOULEX_ITERATION\" = \"1\" ] || [ \"$JOULEX_ITERATION\" = \"3\" ]; then exit 1; else sleep 0.01; fi'")
        .assert()
        .success();

    let contents = std::fs::read_to_string(export_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
    let result = &parsed["results"][0];
    let times = result["times"].as_array().unwrap();
    let exit_codes = result["exit_codes"].as_array().unwrap();
    assert_eq!(times.len(), 3); // 5 runs - 2 failed = 3 kept
    assert_eq!(exit_codes.len(), 3);
    for code in exit_codes {
        assert_eq!(code, &serde_json::json!(0));
    }

    let omitted = result["omitted_failed_runs"].as_array().unwrap();
    assert_eq!(omitted.len(), 2);
    assert_eq!(omitted[0]["index"], 1);
    assert_eq!(omitted[0]["exit_code"], 1);
    assert_eq!(omitted[1]["index"], 3);
    assert_eq!(omitted[1]["exit_code"], 1);
}

#[cfg(unix)]
#[test]
fn omit_failed_runs_with_filter_failed_preserves_successful_runs() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("results.json");

    hyperfine()
        .arg("-i")
        .arg("--omit-failed-runs")
        .arg("--filter-failed")
        .arg("-r=6")
        .arg("--export-json")
        .arg(&export_path)
        .arg("sh -c 'if [ \"$JOULEX_ITERATION\" = \"2\" ]; then exit 1; else sleep 0.01; fi'")
        .arg("sleep 0.02")
        .assert()
        .success();

    let contents = std::fs::read_to_string(export_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
    let results = parsed["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);

    let first_cmd = &results[0];
    let times = first_cmd["times"].as_array().unwrap();
    let exit_codes = first_cmd["exit_codes"].as_array().unwrap();
    assert_eq!(times.len(), 5);
    assert_eq!(exit_codes.len(), 5);
    for code in exit_codes {
        assert_eq!(code, &serde_json::json!(0));
    }
    let omitted = first_cmd["omitted_failed_runs"].as_array().unwrap();
    assert_eq!(omitted.len(), 1);
    assert_eq!(omitted[0]["index"], 2);
    assert_eq!(omitted[0]["exit_code"], 1);

    let second_cmd = &results[1];
    assert!(second_cmd.get("omitted_failed_runs").is_none());
}

#[test]
fn test_short_flags_style_and_sort() {
    hyperfine()
        .arg("-r=2")
        .arg("-l")
        .arg("basic")
        .arg("-t")
        .arg("command")
        .arg("echo a")
        .arg("echo b")
        .assert()
        .success();
}

#[test]
fn test_short_flag_show_output() {
    hyperfine()
        .arg("-r=2")
        .arg("-d")
        .arg("echo output_marker_123")
        .assert()
        .success()
        .stdout(predicate::str::contains("output_marker_123"));
}

#[test]
fn test_short_flag_output_and_input() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let input_path = tempdir.path().join("input.txt");
    let output_path = tempdir.path().join("output.txt");

    std::fs::write(&input_path, "hello short flags\n").unwrap();

    hyperfine()
        .arg("-r=2")
        .arg("-I")
        .arg(&input_path)
        .arg("-O")
        .arg(&output_path)
        .arg("cat")
        .assert()
        .success();

    let output_content = std::fs::read_to_string(&output_path).unwrap();
    assert!(output_content.contains("hello short flags"));
}

#[test]
fn test_deep_stats_identical_constant_samples() {
    use predicates::prelude::PredicateBooleanExt;
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let json_path = tempdir.path().join("const.json");

    let json_content = r#"{
        "results": [
            {
                "command": "cmd_a",
                "mean": 1.0,
                "stddev": 0.0,
                "median": 1.0,
                "user": 0.0,
                "system": 0.0,
                "min": 1.0,
                "max": 1.0,
                "times": [1.0, 1.0, 1.0, 1.0],
                "exit_codes": [0, 0, 0, 0]
            },
            {
                "command": "cmd_b",
                "mean": 1.0,
                "stddev": 0.0,
                "median": 1.0,
                "user": 0.0,
                "system": 0.0,
                "min": 1.0,
                "max": 1.0,
                "times": [1.0, 1.0, 1.0, 1.0],
                "exit_codes": [0, 0, 0, 0]
            }
        ]
    }"#;
    std::fs::write(&json_path, json_content).unwrap();

    hyperfine()
        .arg("--import-json")
        .arg(&json_path)
        .arg("--deep-stats")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "p = 1.0000 -> no statistically significant difference",
        ))
        .stdout(predicate::str::contains("t = NaN").not())
        .stdout(predicate::str::contains("significant (p < 0.01)").not());
}

#[test]
fn help_has_no_colors_when_not_a_terminal() {
    let output = hyperfine().arg("--help").output().unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains('\u{1b}'));
}

#[test]
fn help_respects_no_color_even_when_colors_are_forced() {
    let output = hyperfine()
        .env("CLICOLOR_FORCE", "1")
        .env("NO_COLOR", "1")
        .arg("--help")
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains('\u{1b}'));
}

#[test]
fn help_is_colored_when_colors_are_forced() {
    let output = hyperfine()
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .arg("--help")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // bold green section headers, e.g. "Usage:" / "Arguments:"
    assert!(stdout.contains("\u{1b}[1m\u{1b}[32m") || stdout.contains("\u{1b}[1;32m"));
}

#[test]
fn parameter_scan_cartesian_product_exceeding_limit_fails_fast() {
    hyperfine()
        .arg("-N")
        .arg("-r=1")
        .arg("-P")
        .arg("a")
        .arg("1")
        .arg("20000")
        .arg("-P")
        .arg("b")
        .arg("1")
        .arg("20000")
        .arg("echo {a} {b}")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The parameter combinations would create more than 100000 benchmarks",
        ));
}

#[test]
fn imported_command_names_cannot_inject_terminal_escape_sequences() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let import_path = tempdir.path().join("evil.json");
    let markdown_path = tempdir.path().join("evil.md");
    std::fs::write(
        &import_path,
        r#"{"results":[{"command":"\u001b]0;PWNED\u0007\u001b[2Jevil","mean":1,"stddev":0.1,"median":1,"user":0,"system":0,"min":1,"max":1,"times":[1],"exit_codes":[0],"parameters":{"p":"\u001b[31mred"}}]}"#,
    )
    .unwrap();

    let output = hyperfine()
        .arg("--import-json")
        .arg(&import_path)
        .arg("--export-markdown")
        .arg(&markdown_path)
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains('\u{1b}'), "raw ESC leaked to the terminal");
    assert!(stdout.contains("\\u{1b}]0;PWNED\\u{7}\\u{1b}[2Jevil (imported)"));

    let markdown = std::fs::read_to_string(&markdown_path).unwrap();
    assert!(!markdown.contains('\u{1b}'), "raw ESC leaked to the export");
}

#[cfg(unix)]
#[test]
fn own_exports_pass_import_validation_including_omitted_failed_runs() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("omitted.json");

    hyperfine()
        .arg("--runs=4")
        .arg("--ignore-failure")
        .arg("--omit-failed-runs")
        .arg("--export-json")
        .arg(&export_path)
        .arg("sh -c '[ \"$JOULEX_ITERATION\" = 2 ] && exit 1; exit 0'")
        .assert()
        .success();

    hyperfine()
        .arg("--import-json")
        .arg(&export_path)
        .assert()
        .success();
}

#[test]
fn fails_to_import_json_with_invalid_results() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let import_path = tempdir.path().join("invalid.json");
    std::fs::write(
        &import_path,
        r#"{"results":[{"command":"a","mean":-1,"stddev":0,"median":1,"user":0,"system":0,"min":0,"max":1}]}"#,
    )
    .unwrap();

    hyperfine()
        .arg("--import-json")
        .arg(&import_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Invalid benchmark result #1 ('a')",
        ))
        .stderr(predicate::str::contains(
            "'mean' must be a finite, non-negative number",
        ));
}

#[test]
fn round_robin_rejects_parametrized_setup() {
    hyperfine_debug()
        .arg("--schedule=round-robin")
        .arg("-P")
        .arg("val")
        .arg("1")
        .arg("2")
        .arg("--setup=sleep 0.00{val}")
        .arg("sleep 0.1")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The '--setup' and/or '--cleanup' options differ between benchmarks (due to parameter substitution) and cannot be combined with '--schedule round-robin'.",
        ));
}

#[test]
fn round_robin_rejects_parametrized_cleanup() {
    hyperfine_debug()
        .arg("--schedule=round-robin")
        .arg("-P")
        .arg("val")
        .arg("1")
        .arg("2")
        .arg("--cleanup=sleep 0.00{val}")
        .arg("sleep 0.1")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "The '--setup' and/or '--cleanup' options differ between benchmarks (due to parameter substitution) and cannot be combined with '--schedule round-robin'.",
        ));
}

#[test]
fn round_robin_allows_parametrized_setup_with_override_flag() {
    hyperfine_debug()
        .arg("--schedule=round-robin")
        .arg("--allow-setup-with-round-robin")
        .arg("-r=1")
        .arg("-P")
        .arg("val")
        .arg("1")
        .arg("2")
        .arg("--setup=sleep 0.00{val}")
        .arg("sleep 0.1")
        .assert()
        .success();
}

#[test]
fn round_robin_allows_unparametrized_setup() {
    hyperfine_debug()
        .arg("--schedule=round-robin")
        .arg("-r=1")
        .arg("-P")
        .arg("val")
        .arg("1")
        .arg("2")
        .arg("--setup=sleep 0.001")
        .arg("sleep 0.1")
        .assert()
        .success();
}

#[test]
fn fish_completions_complete_executables_for_the_command() {
    hyperfine()
        .arg("--generate-completions")
        .arg("fish")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "complete -c joulex -n \"__fish_use_subcommand\" -x -a \"(__fish_complete_command)\"",
        ));
}

#[test]
fn non_fish_completions_do_not_contain_fish_snippet() {
    hyperfine()
        .arg("--generate-completions")
        .arg("bash")
        .assert()
        .success()
        .stdout(predicate::str::contains("__fish_complete_command").not());
}

#[test]
fn existing_export_is_kept_when_benchmark_fails() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("previous.json");
    std::fs::write(&export_path, "previous results").unwrap();

    hyperfine_debug()
        .arg("--export-json")
        .arg(&export_path)
        .arg("--prepare=exit 1")
        .arg("sleep 0.1")
        .assert()
        .failure();

    assert_eq!(
        std::fs::read_to_string(&export_path).unwrap(),
        "previous results"
    );
}

#[test]
fn export_to_missing_directory_fails_before_benchmarking() {
    hyperfine_debug()
        .arg("--export-json")
        .arg("/nonexistent-joulex-dir/out.json")
        .arg("sleep 0.1")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Could not create export file '/nonexistent-joulex-dir/out.json'",
        ));
}

#[cfg(unix)]
#[test]
fn export_replaces_symlink_instead_of_writing_through_it() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let victim = tempdir.path().join("victim.txt");
    std::fs::write(&victim, "do not overwrite").unwrap();
    let link = tempdir.path().join("results.json");
    std::os::unix::fs::symlink(&victim, &link).unwrap();

    hyperfine_debug()
        .arg("--export-json")
        .arg(&link)
        .arg("sleep 0.1")
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(&victim).unwrap(),
        "do not overwrite"
    );
    let metadata = std::fs::symlink_metadata(&link).unwrap();
    assert!(metadata.file_type().is_file());
    assert!(std::fs::read_to_string(&link)
        .unwrap()
        .contains("\"results\""));
    // no temporary files are left behind
    assert_eq!(std::fs::read_dir(tempdir.path()).unwrap().count(), 2);
}

#[test]
fn round_robin_equal_run_counts_for_asymmetric_commands() {
    use tempfile::tempdir;

    let tempdir = tempdir().unwrap();
    let export_path = tempdir.path().join("results.json");

    hyperfine_debug()
        .arg("--schedule=round-robin")
        .arg("--export-json")
        .arg(&export_path)
        .arg("sleep 0.005")
        .arg("sleep 0.05")
        .assert()
        .success();

    let contents = std::fs::read_to_string(export_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
    let results = parsed["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    let count_fast = results[0]["times"].as_array().unwrap().len();
    let count_slow = results[1]["times"].as_array().unwrap().len();
    assert_eq!(
        count_fast, count_slow,
        "Fast command runs ({count_fast}) must equal slow command runs ({count_slow}) in round-robin mode"
    );
}

#[test]
fn schedule_interleaved_alias_works() {
    hyperfine_debug()
        .arg("--schedule=interleaved")
        .arg("-r=2")
        .arg("sleep 0.01")
        .arg("sleep 0.02")
        .assert()
        .success();
}

#[test]
fn schedule_sequential_is_rejected() {
    hyperfine_debug()
        .arg("--schedule=sequential")
        .arg("sleep 0.01")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "invalid value 'sequential' for '--schedule <MODE>'",
        ));
}

#[cfg(unix)]
#[test]
fn iteration_placeholder_works_without_a_shell() {
    hyperfine()
        .arg("--shell=none")
        .arg("--warmup=1")
        .arg("--runs=3")
        .arg("--show-output")
        .arg("echo run-{iteration}")
        .assert()
        .success()
        .stdout(predicate::str::contains("run-warmup-0"))
        .stdout(predicate::str::contains("run-0"))
        .stdout(predicate::str::contains("run-2"))
        // the header shows the template, but no run prints it unexpanded
        .stdout(predicate::str::contains(
            "Benchmark 1: echo run-{iteration}",
        ))
        .stdout(predicate::str::contains("\nrun-{iteration}\n").not());
}

#[cfg(unix)]
#[test]
fn iteration_placeholder_is_expanded_in_prepare() {
    hyperfine()
        .arg("--runs=2")
        .arg("--prepare=echo prep-{iteration}")
        .arg("--show-output")
        .arg("true")
        .assert()
        .success()
        .stdout(predicate::str::contains("prep-0"))
        .stdout(predicate::str::contains("prep-1"));
}

#[test]
fn deep_stats_terminal_output_respects_time_unit_and_shows_stddev() {
    hyperfine_debug()
        .arg("--deep-stats")
        .arg("--time-unit=millisecond")
        .arg("-r=5")
        .arg("sleep 0.05")
        .assert()
        .success()
        .stdout(predicate::str::contains("Bootstrap 95% CI:"))
        .stdout(predicate::str::contains("ms"))
        .stdout(predicate::str::contains("σ:"));
}

#[cfg(unix)]
#[test]
fn export_to_dev_stdout_and_dev_null_works() {
    // Regression test: the atomic write-and-rename of exports must not be used
    // for devices (renaming into /dev is not permitted).
    hyperfine()
        .args([
            "--runs=1",
            "--style=none",
            "--export-csv=/dev/stdout",
            "echo",
            "true",
        ])
        .assert()
        .success()
        // written once, by the final export (not after every benchmark)
        .stdout(predicate::str::contains("command,mean,stddev").count(1));

    hyperfine()
        .args([
            "--runs=1",
            "--style=none",
            "--export-json=/dev/null",
            "echo",
        ])
        .assert()
        .success();
}

#[cfg(unix)]
#[test]
fn export_to_a_fifo_works() {
    use std::io::Read;

    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("results.fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(status.success());

    let reader = {
        let fifo = fifo.clone();
        std::thread::spawn(move || {
            let mut content = String::new();
            std::fs::File::open(fifo)
                .unwrap()
                .read_to_string(&mut content)
                .unwrap();
            content
        })
    };

    hyperfine()
        .args(["--runs=1", "--style=none", "--export-json"])
        .arg(&fifo)
        .arg("echo")
        .assert()
        .success();

    assert!(reader.join().unwrap().contains("\"results\""));
    // the FIFO itself is still there (not replaced by a regular file)
    use std::os::unix::fs::FileTypeExt;
    assert!(std::fs::metadata(&fifo).unwrap().file_type().is_fifo());
}
