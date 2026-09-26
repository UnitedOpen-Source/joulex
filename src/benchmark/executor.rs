#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::ExitStatus;

use crate::command::Command;
use crate::options::{
    CmdFailureAction, CommandInputPolicy, CommandOutputPolicy, Options, OutputStyleOption, Shell,
};
use crate::output::progress_bar::get_progress_bar;
use crate::timer::{execute_and_measure, TimerResult};
use crate::util::priority::Priority;
use crate::util::randomized_environment_offset;
use crate::util::units::Second;

use super::timing_result::TimingResult;

use crate::stats::basic::mean;
use anyhow::{anyhow, bail, Context, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkIteration {
    NonBenchmarkRun,
    Warmup(u64),
    Benchmark(u64),
}

impl BenchmarkIteration {
    pub fn to_env_var_value(&self) -> Option<String> {
        match self {
            BenchmarkIteration::NonBenchmarkRun => None,
            BenchmarkIteration::Warmup(i) => Some(format!("warmup-{i}")),
            BenchmarkIteration::Benchmark(i) => Some(format!("{i}")),
        }
    }
}

pub trait Executor {
    /// Run the given command and measure the execution time
    fn run_command_and_measure(
        &self,
        command: &Command<'_>,
        iteration: BenchmarkIteration,
        command_failure_action: Option<CmdFailureAction>,
        output_policy: &CommandOutputPolicy,
    ) -> Result<(TimingResult, ExitStatus)>;

    /// Perform a calibration of this executor. For example,
    /// when running commands through a shell, we need to
    /// measure the shell spawning time separately in order
    /// to subtract it from the full runtime later.
    fn calibrate(&mut self) -> Result<()>;

    /// Return the time overhead for this executor when
    /// performing a measurement. This should return the time
    /// that is being used in addition to the actual runtime
    /// of the command.
    fn time_overhead(&self) -> Second;

    /// Whether commands run through an intermediate shell (whose spawning
    /// time is subtracted, which limits the accuracy for very fast commands)
    fn uses_shell(&self) -> bool {
        false
    }
}

/// How every benchmarked (and intermediate) process is started
#[derive(Clone, Copy)]
struct ProcessSettings<'a> {
    /// --affinity
    affinity: Option<&'a [usize]>,
    /// --priority
    priority: Priority,
    /// --timeout
    timeout: Option<std::time::Duration>,
    /// --until
    until: Option<&'a crate::options::UntilSettings>,
    /// --output-metric
    output_metrics: &'a [crate::output_metric::OutputMetric],
}

impl<'a> ProcessSettings<'a> {
    fn from_options(options: &'a Options) -> Self {
        ProcessSettings {
            affinity: options.affinity.as_deref(),
            priority: options.priority,
            timeout: options.timeout,
            until: options.until.as_ref(),
            output_metrics: &options.output_metrics,
        }
    }
}

fn run_command_and_measure_common(
    mut command: std::process::Command,
    iteration: BenchmarkIteration,
    command_failure_action: CmdFailureAction,
    command_input_policy: &CommandInputPolicy,
    command_output_policy: &CommandOutputPolicy,
    command_name: &str,
    process: ProcessSettings,
) -> Result<TimerResult> {
    let ProcessSettings {
        affinity,
        priority,
        timeout,
        until,
        output_metrics,
    } = process;
    let (until, capture_metrics) = match iteration {
        BenchmarkIteration::NonBenchmarkRun => (None, false),
        BenchmarkIteration::Warmup(_) | BenchmarkIteration::Benchmark(_) => {
            (until, !output_metrics.is_empty())
        }
    };

    let stdin = command_input_policy.get_stdin()?;
    let (stdout, stderr) = match until {
        Some(u) if !u.match_stderr => (std::process::Stdio::piped(), std::process::Stdio::null()),
        Some(_) => (std::process::Stdio::null(), std::process::Stdio::piped()),
        None => {
            let (mut out, err) = command_output_policy.get_stdout_stderr()?;
            if capture_metrics {
                out = std::process::Stdio::piped();
            }
            (out, err)
        }
    };
    command.stdin(stdin).stdout(stdout).stderr(stderr);

    command.env(
        "HYPERFINE_RANDOMIZED_ENVIRONMENT_OFFSET",
        randomized_environment_offset::value(),
    );

    if let Some(value) = iteration.to_env_var_value() {
        command.env("JOULEX_ITERATION", &value);
        command.env("HYPERFINE_ITERATION", value);
    }

    #[cfg(target_os = "linux")]
    if let Some(cpus) = affinity {
        crate::util::affinity::apply(&mut command, cpus);
    }

    #[cfg(unix)]
    crate::util::priority::apply(&mut command, priority);

    let interrupted_before = crate::util::interrupt::interrupted();
    let capture = *command_output_policy == CommandOutputPolicy::CaptureTail;
    let stdout_capture_limit = if capture_metrics {
        Some(1 << 20)
    } else if capture {
        Some(crate::timer::CAPTURE_LIMIT)
    } else {
        None
    };
    let mut result = execute_and_measure(
        command,
        affinity,
        priority,
        capture,
        timeout,
        until,
        stdout_capture_limit,
    )
    .map_err(|error| {
        // A priority that needs privileges fails in the child: explain how
        // to get them
        let denied = error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied);
        match crate::util::priority::permission_hint(priority) {
            Some(hint) if denied => error.context(format!(
                "could not set '--priority {}': {hint}",
                priority.as_str()
            )),
            _ => error,
        }
    })
    .with_context(|| format!("Failed to run command '{command_name}'"))?;

    // If an interruption occurred while running the command, discard the run
    // regardless of whether the child exited 0 (e.g. child caught SIGINT and exited gracefully).
    if !interrupted_before && crate::util::interrupt::interrupted() {
        bail!(crate::error::Interrupted);
    }

    if !result.status.success() {
        if crate::util::interrupt::interrupted() {
            bail!(crate::error::Interrupted);
        }

        // If the process was terminated by the timeout watchdog, return Ok(result)
        // so callers can handle the timeout cleanly without failing as an unhandled error.
        if result.timed_out {
            return Ok(result);
        }

        use crate::util::exit_code::extract_exit_code;

        let should_fail = match command_failure_action {
            CmdFailureAction::RaiseError => true,
            CmdFailureAction::IgnoreAllFailures => false,
            CmdFailureAction::IgnoreSpecificFailures(ref codes) => {
                // Only fail if the exit code is not in the list of codes to ignore
                if let Some(exit_code) = extract_exit_code(result.status) {
                    !codes.contains(&exit_code)
                } else {
                    // If we can't extract an exit code, treat it as a failure
                    true
                }
            }
        };

        if should_fail {
            let when = match iteration {
                BenchmarkIteration::NonBenchmarkRun => "a non-benchmark run".to_string(),
                BenchmarkIteration::Warmup(0) => "the first warmup run".to_string(),
                BenchmarkIteration::Warmup(i) => format!("warmup iteration {i}"),
                BenchmarkIteration::Benchmark(0) => "the first benchmark run".to_string(),
                BenchmarkIteration::Benchmark(i) => format!("benchmark iteration {i}"),
            };
            let cause = if result.until_matched == Some(false) {
                "Command exited without matching '--until' pattern".to_string()
            } else {
                result
                    .status
                    .code()
                    .map_or("The process has been terminated by a signal".into(), |c| {
                        format!("Command terminated with non-zero exit code {c}")
                    })
            };
            match &result.captured {
                Some(captured) => bail!(
                    "{cause} in {when}. Use the '-i'/'--ignore-exit-code' option if you want \
                     to ignore this.\n{}",
                    format_captured_output(captured)
                ),
                None => bail!(
                    "{cause} in {when}. Use the '-i'/'--ignore-exit-code' option if you want \
                     to ignore this. Alternatively, use the '--show-output-on-failure' (or \
                     '--show-output') option to debug what went wrong."
                ),
            }
        } else if let Some(captured) = &result.captured {
            warn_failed_run_output(command_name, iteration, captured);
        }
    }

    if capture_metrics {
        let stdout = result
            .captured
            .as_ref()
            .map(|c| c.stdout.as_slice())
            .unwrap_or(&[]);
        for metric in output_metrics {
            if let Some(val) = metric.extract(stdout) {
                result.custom_metrics.insert(metric.name.clone(), val);
            } else {
                let should_fail = match command_failure_action {
                    CmdFailureAction::RaiseError => true,
                    CmdFailureAction::IgnoreAllFailures => false,
                    CmdFailureAction::IgnoreSpecificFailures(_) => true,
                };
                let when = match iteration {
                    BenchmarkIteration::NonBenchmarkRun => "a non-benchmark run".to_string(),
                    BenchmarkIteration::Warmup(0) => "the first warmup run".to_string(),
                    BenchmarkIteration::Warmup(i) => format!("warmup iteration {i}"),
                    BenchmarkIteration::Benchmark(0) => "the first benchmark run".to_string(),
                    BenchmarkIteration::Benchmark(i) => format!("benchmark iteration {i}"),
                };
                if should_fail {
                    bail!(
                        "Metric '{}' could not be extracted from output in {when}. Use the '-i'/'--ignore-failure' option if you want to ignore this.",
                        metric.name
                    );
                } else {
                    eprintln!(
                        "{} metric '{}' could not be extracted from output in {when} (ignored).",
                        crate::output::colors::yellow("Warning:"),
                        metric.name
                    );
                }
            }
        }
    }

    Ok(result)
}

/// Number of lines of each stream shown for a failed run
const CAPTURED_LINES: usize = 20;
/// Longer lines (e.g. binary output) are cut to their last this many
/// characters (the end of the output is usually the interesting part)
const CAPTURED_LINE_CHARS: usize = 300;
/// With '-i', the output of at most this many failed runs is shown
const MAX_FAILED_RUN_OUTPUTS: usize = 3;

/// The last lines of a captured stream, with control characters escaped
/// (the output is untrusted and must not control the terminal).
fn tail_lines(bytes: &[u8]) -> Option<(usize, String)> {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().collect();
    if lines.iter().all(|line| line.trim().is_empty()) {
        return None;
    }
    let tail = &lines[lines.len().saturating_sub(CAPTURED_LINES)..];
    let shown = tail
        .iter()
        .map(|line| {
            let chars = line.chars().count();
            let cut = chars.saturating_sub(CAPTURED_LINE_CHARS);
            let line: String = line.chars().skip(cut).collect();
            let line = crate::util::sanitize::escape_control_chars(&line).into_owned();
            if cut > 0 {
                format!("({cut} earlier characters) … {line}")
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Some((tail.len(), shown))
}

/// The captured stderr and stdout of a failed run, for an error or warning.
fn format_captured_output(captured: &crate::timer::CapturedOutput) -> String {
    let mut out = String::new();
    for (name, bytes) in [("stderr", &captured.stderr), ("stdout", &captured.stdout)] {
        if let Some((count, lines)) = tail_lines(bytes) {
            out.push_str(&format!("──── {name} (last {count} lines) ────\n{lines}\n"));
        }
    }
    if out.is_empty() {
        out.push_str("(the command produced no output)\n");
    }
    out
}

/// '-i' with '--show-output-on-failure': show the output of the first failed
/// runs as a warning and continue.
fn warn_failed_run_output(
    command_name: &str,
    iteration: BenchmarkIteration,
    captured: &crate::timer::CapturedOutput,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SHOWN: AtomicUsize = AtomicUsize::new(0);

    match SHOWN.fetch_add(1, Ordering::Relaxed) {
        shown if shown < MAX_FAILED_RUN_OUTPUTS => {
            let run = match iteration {
                BenchmarkIteration::Benchmark(i) => format!("benchmark iteration {i}"),
                BenchmarkIteration::Warmup(i) => format!("warmup iteration {i}"),
                BenchmarkIteration::NonBenchmarkRun => "a non-benchmark run".to_string(),
            };
            eprintln!(
                "{} '{}' failed in {run} (ignored):\n{}",
                crate::output::colors::yellow("Warning:"),
                crate::util::sanitize::escape_control_chars(command_name),
                format_captured_output(captured)
            );
        }
        MAX_FAILED_RUN_OUTPUTS => eprintln!(
            "{} the output of further failed runs is not shown.",
            crate::output::colors::yellow("Warning:")
        ),
        _ => {}
    }
}

pub struct RawExecutor<'a> {
    options: &'a Options,
}

impl<'a> RawExecutor<'a> {
    pub fn new(options: &'a Options) -> Self {
        RawExecutor { options }
    }
}

impl Executor for RawExecutor<'_> {
    fn run_command_and_measure(
        &self,
        command: &Command<'_>,
        iteration: BenchmarkIteration,
        command_failure_action: Option<CmdFailureAction>,
        output_policy: &CommandOutputPolicy,
    ) -> Result<(TimingResult, ExitStatus)> {
        let command = command.for_iteration(&iteration);
        let result = run_command_and_measure_common(
            command.get_command()?,
            iteration,
            command_failure_action.unwrap_or_else(|| self.options.command_failure_action.clone()),
            &self.options.command_input_policy,
            output_policy,
            &command.get_command_line(),
            ProcessSettings::from_options(self.options),
        )?;

        Ok((
            TimingResult {
                time_real: result.time_real,
                time_user: result.time_user,
                time_system: result.time_system,
                memory_usage_byte: result.memory_usage_byte,
                energy_joules: None,
                counters: result.counters,
                timed_out: result.timed_out,
                custom_metrics: result.custom_metrics,
            },
            result.status,
        ))
    }

    fn calibrate(&mut self) -> Result<()> {
        Ok(())
    }

    fn time_overhead(&self) -> Second {
        0.0
    }
}

pub struct ShellExecutor<'a> {
    options: &'a Options,
    shell: &'a Shell,
    shell_spawning_time: Option<TimingResult>,
}

impl<'a> ShellExecutor<'a> {
    pub fn new(shell: &'a Shell, options: &'a Options) -> Self {
        ShellExecutor {
            shell,
            options,
            shell_spawning_time: None,
        }
    }
}

/// `cmd.exe /C ./app.exe` fails ("'.' is not recognized as an internal or
/// external command"): cmd only understands backslashes in the program path.
/// Rewrite the forward slashes of a relative program path (`./…`, `../…`) to
/// backslashes. The arguments are left untouched.
#[cfg(any(windows, test))]
fn normalize_relative_command_path_for_cmd(command_line: &str) -> String {
    if !(command_line.starts_with("./") || command_line.starts_with("../")) {
        return command_line.to_string();
    }
    let end = command_line
        .find(char::is_whitespace)
        .unwrap_or(command_line.len());
    format!(
        "{}{}",
        command_line[..end].replace('/', "\\"),
        &command_line[end..]
    )
}

impl Executor for ShellExecutor<'_> {
    fn run_command_and_measure(
        &self,
        command: &Command<'_>,
        iteration: BenchmarkIteration,
        command_failure_action: Option<CmdFailureAction>,
        output_policy: &CommandOutputPolicy,
    ) -> Result<(TimingResult, ExitStatus)> {
        let command = command.for_iteration(&iteration);
        let on_windows_cmd = cfg!(windows) && *self.shell == Shell::Default("cmd.exe");
        let mut command_builder = self.shell.command();
        command_builder.arg(if on_windows_cmd { "/C" } else { "-c" });

        // Windows needs special treatment for its behavior on parsing cmd arguments
        if on_windows_cmd {
            #[cfg(windows)]
            command_builder.raw_arg(normalize_relative_command_path_for_cmd(
                &command.get_command_line(),
            ));
        } else {
            command_builder.arg(command.get_command_line());
        }

        let mut result = run_command_and_measure_common(
            command_builder,
            iteration,
            command_failure_action.unwrap_or_else(|| self.options.command_failure_action.clone()),
            &self.options.command_input_policy,
            output_policy,
            &command.get_command_line(),
            ProcessSettings::from_options(self.options),
        )?;

        // Subtract shell spawning time
        if let Some(ref spawning_time) = self.shell_spawning_time {
            result.time_real = (result.time_real - spawning_time.time_real).max(0.0);
            result.time_user = (result.time_user - spawning_time.time_user).max(0.0);
            result.time_system = (result.time_system - spawning_time.time_system).max(0.0);
        }

        Ok((
            TimingResult {
                time_real: result.time_real,
                time_user: result.time_user,
                time_system: result.time_system,
                memory_usage_byte: result.memory_usage_byte,
                energy_joules: None,
                counters: result.counters,
                timed_out: result.timed_out,
                custom_metrics: result.custom_metrics,
            },
            result.status,
        ))
    }

    /// Measure the average shell spawning time
    fn calibrate(&mut self) -> Result<()> {
        const COUNT: u64 = 50;
        let progress_bar = if self.options.output_style != OutputStyleOption::Disabled {
            Some(get_progress_bar(
                COUNT,
                "Measuring shell spawning time",
                self.options.output_style,
            ))
        } else {
            None
        };

        let mut times_real: Vec<Second> = vec![];
        let mut times_user: Vec<Second> = vec![];
        let mut times_system: Vec<Second> = vec![];

        for _ in 0..COUNT {
            // Just run the shell without any command
            let res = self.run_command_and_measure(
                &Command::new(None, ""),
                BenchmarkIteration::NonBenchmarkRun,
                None,
                &CommandOutputPolicy::Null,
            );

            match res {
                Err(error) => {
                    let shell_cmd = if cfg!(windows) {
                        format!("{} /C \"\"", self.shell)
                    } else {
                        format!("{} -c \"\"", self.shell)
                    };

                    // Keep the cause (e.g. a '--priority' permission error)
                    return Err(error.context(format!(
                        "Could not measure the shell execution time (make sure you can run '{shell_cmd}')"
                    )));
                }
                Ok((r, _)) => {
                    times_real.push(r.time_real);
                    times_user.push(r.time_user);
                    times_system.push(r.time_system);
                }
            }

            if let Some(bar) = progress_bar.as_ref() {
                bar.inc(1)
            }
        }

        if let Some(bar) = progress_bar.as_ref() {
            bar.finish_and_clear()
        }

        self.shell_spawning_time = Some(TimingResult {
            time_real: mean(&times_real),
            time_user: mean(&times_user),
            time_system: mean(&times_system),
            memory_usage_byte: 0,
            energy_joules: None,
            counters: None,
            timed_out: false,
            custom_metrics: std::collections::BTreeMap::new(),
        });

        Ok(())
    }

    fn uses_shell(&self) -> bool {
        true
    }

    fn time_overhead(&self) -> Second {
        // Zero before `calibrate()` has measured the shell spawning time
        self.shell_spawning_time
            .as_ref()
            .map_or(0.0, |t| t.time_real)
    }
}

#[derive(Clone)]
pub struct MockExecutor {
    shell: Option<String>,
    timeout: Option<std::time::Duration>,
}

impl MockExecutor {
    pub fn new(shell: Option<String>, timeout: Option<std::time::Duration>) -> Self {
        MockExecutor { shell, timeout }
    }

    /// `--debug-mode` doesn't run anything: it only understands commands of
    /// the form `sleep <seconds>` and reports exactly that time.
    fn extract_time<S: AsRef<str>>(sleep_command: S) -> Result<Second> {
        let command = sleep_command.as_ref();
        command
            .strip_prefix("sleep ")
            .and_then(|seconds| seconds.trim().parse::<Second>().ok())
            .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
            .ok_or_else(|| {
                anyhow!(
                    "'--debug-mode' only simulates commands of the form 'sleep <seconds>', \
                     got '{command}'"
                )
            })
    }
}

impl Executor for MockExecutor {
    fn run_command_and_measure(
        &self,
        command: &Command<'_>,
        iteration: BenchmarkIteration,
        _command_failure_action: Option<CmdFailureAction>,
        _output_policy: &CommandOutputPolicy,
    ) -> Result<(TimingResult, ExitStatus)> {
        let command = command.for_iteration(&iteration);
        #[cfg(unix)]
        let status = {
            use std::os::unix::process::ExitStatusExt;
            ExitStatus::from_raw(0)
        };

        #[cfg(windows)]
        let status = {
            use std::os::windows::process::ExitStatusExt;
            ExitStatus::from_raw(0)
        };

        let requested = Self::extract_time(command.get_command_line())?;
        let timed_out = self.timeout.is_some_and(|t| requested > t.as_secs_f64());
        let time_real = if timed_out {
            self.timeout.unwrap().as_secs_f64()
        } else {
            requested
        };

        Ok((
            TimingResult {
                time_real,
                time_user: 0.0,
                time_system: 0.0,
                memory_usage_byte: 0,
                energy_joules: None,
                counters: None,
                timed_out,
                custom_metrics: std::collections::BTreeMap::new(),
            },
            status,
        ))
    }

    fn calibrate(&mut self) -> Result<()> {
        // Validate the simulated shell (`--debug-mode --shell 'sleep …'`), so
        // that `time_overhead` can't fail later
        if let Some(shell) = &self.shell {
            Self::extract_time(shell)?;
        }
        Ok(())
    }

    fn time_overhead(&self) -> Second {
        self.shell
            .as_ref()
            .and_then(|shell| Self::extract_time(shell).ok())
            .unwrap_or(0.0)
    }
}

#[test]
fn test_mock_executor_extract_time() {
    assert_eq!(MockExecutor::extract_time("sleep 0.1").unwrap(), 0.1);
    for invalid in ["echo hi", "sleep", "sleep abc", "sleep -1", "sleep inf"] {
        assert!(MockExecutor::extract_time(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn test_normalize_relative_command_path_for_cmd() {
    for (input, expected) in [
        ("./app.exe", ".\\app.exe"),
        (
            "./target/release/app.exe --x a/b",
            ".\\target\\release\\app.exe --x a/b",
        ),
        ("../bin/app.exe\t./x", "..\\bin\\app.exe\t./x"),
        ("app.exe ./x", "app.exe ./x"),
        ("C:/tools/app.exe", "C:/tools/app.exe"),
        (".hidden/app", ".hidden/app"),
        ("", ""),
    ] {
        assert_eq!(normalize_relative_command_path_for_cmd(input), expected);
    }
}

#[test]
fn captured_output_is_tailed_escaped_and_cut() {
    let many_lines: String = (1..=30).map(|i| format!("line {i}\n")).collect();
    let (count, text) = tail_lines(many_lines.as_bytes()).unwrap();
    assert_eq!(count, CAPTURED_LINES);
    assert!(text.starts_with("line 11\n") && text.ends_with("line 30"));

    let (_, text) = tail_lines(b"\x1b[31mred\x07").unwrap();
    assert_eq!(text, "\\u{1b}[31mred\\u{7}");

    let (_, text) = tail_lines(&[b'x'; 1000]).unwrap();
    assert!(text.starts_with("(700 earlier characters) … x"), "{text}");
    let mut long_line = vec![0u8; 1000];
    long_line.extend_from_slice(b"the error");
    let (_, text) = tail_lines(&long_line).unwrap();
    assert!(text.ends_with("the error"), "{text}");

    assert!(tail_lines(b"").is_none());
    assert!(tail_lines(b"\n  \n").is_none());
    assert_eq!(
        format_captured_output(&crate::timer::CapturedOutput::default()),
        "(the command produced no output)\n"
    );
}
