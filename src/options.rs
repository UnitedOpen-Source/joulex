use std::fs::File;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{cmp, env, fmt, io};

use anyhow::{bail, ensure, Result};
use clap::ArgMatches;

use crate::command::Commands;
use crate::error::OptionsError;
use crate::util::units::{Second, Unit};

#[cfg(not(windows))]
pub const DEFAULT_SHELL: &str = "sh";

#[cfg(windows)]
pub const DEFAULT_SHELL: &str = "cmd.exe";

/// Shell to use for executing benchmarked commands
#[derive(Debug, PartialEq)]
pub enum Shell {
    /// Default shell command
    Default(&'static str),

    /// Custom shell command specified via --shell
    Custom(Vec<String>),
}

impl Default for Shell {
    fn default() -> Self {
        Shell::Default(DEFAULT_SHELL)
    }
}

impl fmt::Display for Shell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Shell::Default(cmd) => write!(f, "{cmd}"),
            Shell::Custom(cmdline) => write!(f, "{}", shell_words::join(cmdline)),
        }
    }
}

impl Shell {
    /// Parse given string as shell command line
    pub fn parse_from_str<'a>(s: &str) -> Result<Self, OptionsError<'a>> {
        let v = shell_words::split(s).map_err(OptionsError::ShellParseError)?;
        if v.is_empty() || v[0].is_empty() {
            return Err(OptionsError::EmptyShell);
        }
        Ok(Shell::Custom(v))
    }

    pub fn command(&self) -> Command {
        match self {
            Shell::Default(cmd) => Command::new(cmd),
            Shell::Custom(cmdline) => {
                let mut c = Command::new(&cmdline[0]);
                c.args(&cmdline[1..]);
                c
            }
        }
    }
}

/// Action to take when an executed command fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdFailureAction {
    /// Exit with an error message
    RaiseError,

    /// Ignore all non-zero exit codes
    IgnoreAllFailures,

    /// Ignore specific exit codes
    IgnoreSpecificFailures(Vec<i32>),
}

/// Output style type option
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStyleOption {
    /// Do not output with colors or any special formatting
    Basic,

    /// Output with full color and formatting
    Full,

    /// Keep elements such as progress bar, but use no coloring
    NoColor,

    /// Keep coloring, but use no progress bar
    Color,

    /// Disable all the output
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Command,
    MeanTime,
}

/// Bounds for the number of benchmark runs
pub struct RunBounds {
    /// Minimum number of benchmark runs
    pub min: u64,

    /// Maximum number of benchmark runs
    pub max: Option<u64>,
}

impl Default for RunBounds {
    fn default() -> Self {
        RunBounds { min: 10, max: None }
    }
}

/// Benchmark execution schedule mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScheduleMode {
    /// Execute all iterations for one command before proceeding to the next
    #[default]
    Grouped,

    /// Interleave iterations across all commands in round-robin fashion
    RoundRobin,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum CommandInputPolicy {
    /// Read from the null device
    #[default]
    Null,

    /// Read input from a file
    File(PathBuf),
}

impl CommandInputPolicy {
    pub fn get_stdin(&self) -> io::Result<Stdio> {
        let stream: Stdio = match self {
            CommandInputPolicy::Null => Stdio::null(),

            CommandInputPolicy::File(path) => {
                let file: File = File::open(path)?;
                Stdio::from(file)
            }
        };

        Ok(stream)
    }
}

/// How to handle the output of benchmarked commands
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CommandOutputPolicy {
    /// Redirect output to the null device
    #[default]
    Null,

    /// Feed output through a pipe before discarding it
    Pipe,

    /// Redirect output to a file
    File(PathBuf),

    /// Show command output on the terminal
    Inherit,
}

impl CommandOutputPolicy {
    pub fn get_stdout_stderr(&self) -> io::Result<(Stdio, Stdio)> {
        let streams = match self {
            CommandOutputPolicy::Null => (Stdio::null(), Stdio::null()),

            // Typically only stdout is performance-relevant, so just pipe that
            CommandOutputPolicy::Pipe => (Stdio::piped(), Stdio::null()),

            CommandOutputPolicy::File(path) => {
                let file = File::create(path)?;
                (file.into(), Stdio::null())
            }

            CommandOutputPolicy::Inherit => (Stdio::inherit(), Stdio::inherit()),
        };

        Ok(streams)
    }
}

#[derive(Debug, PartialEq)]
pub enum ExecutorKind {
    Raw,
    Shell(Shell),
    Mock(Option<String>),
}

impl Default for ExecutorKind {
    fn default() -> Self {
        ExecutorKind::Shell(Shell::default())
    }
}

/// The main settings for a hyperfine benchmark session
pub struct Options {
    /// Upper and lower bound for the number of benchmark runs
    pub run_bounds: RunBounds,

    /// Number of warmup runs
    pub warmup_count: u64,

    /// Minimum benchmarking time
    pub min_benchmarking_time: Second,

    /// Whether or not to ignore non-zero exit codes
    pub command_failure_action: CmdFailureAction,

    // Command to use as a reference for relative speed comparison
    pub reference_command: Option<String>,

    // Name of the reference command
    pub reference_name: Option<String>,

    /// Command(s) to run before each timing run
    pub preparation_command: Option<Vec<String>>,

    /// Command(s) to run after each timing run
    pub conclusion_command: Option<Vec<String>>,

    /// Command to run before each *batch* of timing runs, i.e. before each individual benchmark
    pub setup_command: Option<String>,

    /// Command to run after each *batch* of timing runs, i.e. after each individual benchmark
    pub cleanup_command: Option<String>,

    /// What color mode to use for the terminal output
    pub output_style: OutputStyleOption,

    /// How to order benchmarks in the relative speed comparison
    pub sort_order_speed_comparison: SortOrder,

    /// How to order benchmarks in the markup format exports
    pub sort_order_exports: SortOrder,

    /// Determines how we run commands
    pub executor_kind: ExecutorKind,

    /// Where input to the benchmarked command comes from
    pub command_input_policy: CommandInputPolicy,

    /// What to do with the output of the benchmarked commands
    pub command_output_policies: Vec<CommandOutputPolicy>,

    /// Which time unit to use when displaying results
    pub time_unit: Option<Unit>,

    /// Measure energy consumption in Joules and display Performance per Watt
    pub measure_energy: bool,

    /// Calculate deep statistics (confidence intervals, bootstrapping)
    pub deep_stats: bool,

    /// Whether to exclude results with non-zero exit codes from comparisons and exports
    pub filter_failed: bool,

    /// Suppress statistical outlier warnings
    pub suppress_outlier_warnings: bool,

    /// Execution schedule mode
    pub schedule: ScheduleMode,

    /// Whether to exclude failed runs from summary statistics
    pub omit_failed_runs: bool,

    /// Modified Z-score above which runs are discarded (--discard-outliers)
    pub discard_outliers: Option<f64>,

    /// Show and export OS resource counters (--resource-usage)
    pub show_resource_usage: bool,

    /// Allow combining parametrized '--setup' or '--cleanup' with round-robin scheduling
    pub allow_setup_with_round_robin: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            run_bounds: RunBounds::default(),
            warmup_count: 0,
            min_benchmarking_time: 3.0,
            command_failure_action: CmdFailureAction::RaiseError,
            reference_command: None,
            reference_name: None,
            preparation_command: None,
            conclusion_command: None,
            setup_command: None,
            cleanup_command: None,
            output_style: OutputStyleOption::Full,
            sort_order_speed_comparison: SortOrder::MeanTime,
            sort_order_exports: SortOrder::Command,
            executor_kind: ExecutorKind::default(),
            command_output_policies: vec![CommandOutputPolicy::Null],
            time_unit: None,
            command_input_policy: CommandInputPolicy::Null,
            measure_energy: false,
            deep_stats: false,
            filter_failed: false,
            suppress_outlier_warnings: false,
            schedule: ScheduleMode::Grouped,
            omit_failed_runs: false,
            discard_outliers: None,
            show_resource_usage: false,
            allow_setup_with_round_robin: false,
        }
    }
}

impl Options {
    pub fn from_cli_arguments<'a>(matches: &ArgMatches) -> Result<Self, OptionsError<'a>> {
        let mut options = Self::default();
        let param_to_u64 = |param| {
            matches
                .get_one::<String>(param)
                .map(|n| {
                    n.parse::<u64>()
                        .map_err(|e| OptionsError::IntParsingError(param, e))
                })
                .transpose()
        };

        options.warmup_count = param_to_u64("warmup")?.unwrap_or(options.warmup_count);

        let mut min_runs = param_to_u64("min-runs")?;
        let mut max_runs = param_to_u64("max-runs")?;

        if let Some(runs) = param_to_u64("runs")? {
            if runs == 0 {
                return Err(OptionsError::ZeroRuns("runs"));
            }
            min_runs = Some(runs);
            max_runs = Some(runs);
        } else if max_runs == Some(0) {
            return Err(OptionsError::ZeroRuns("max-runs"));
        }

        match (min_runs, max_runs) {
            (Some(min), None) => {
                options.run_bounds.min = min;
            }
            (None, Some(max)) => {
                // Since the minimum was not explicit we lower it if max is below the default min.
                options.run_bounds.min = cmp::min(options.run_bounds.min, max);
                options.run_bounds.max = Some(max);
            }
            (Some(min), Some(max)) if min > max => {
                return Err(OptionsError::EmptyRunsRange);
            }
            (Some(min), Some(max)) => {
                options.run_bounds.min = min;
                options.run_bounds.max = Some(max);
            }
            (None, None) => {}
        };

        options.setup_command = matches.get_one::<String>("setup").map(String::from);

        options.reference_command = matches.get_one::<String>("reference").map(String::from);
        options.reference_name = matches
            .get_one::<String>("reference-name")
            .map(String::from);

        options.preparation_command = matches
            .get_many::<String>("prepare")
            .map(|values| values.map(String::from).collect::<Vec<String>>());

        options.conclusion_command = matches
            .get_many::<String>("conclude")
            .map(|values| values.map(String::from).collect::<Vec<String>>());

        options.cleanup_command = matches.get_one::<String>("cleanup").map(String::from);

        options.command_output_policies = if matches.get_flag("show-output") {
            vec![CommandOutputPolicy::Inherit]
        } else if let Some(output_values) = matches.get_many::<String>("output") {
            let mut policies = vec![];
            for value in output_values {
                let policy = match value.as_str() {
                    "null" => CommandOutputPolicy::Null,
                    "pipe" => CommandOutputPolicy::Pipe,
                    "inherit" => CommandOutputPolicy::Inherit,
                    arg => {
                        let path = PathBuf::from(arg);
                        if path.components().count() <= 1 {
                            return Err(OptionsError::UnknownOutputPolicy(arg.to_string()));
                        }
                        CommandOutputPolicy::File(path)
                    }
                };
                policies.push(policy);
            }
            policies
        } else {
            vec![CommandOutputPolicy::Null]
        };

        options.output_style = match matches.get_one::<String>("style").map(|s| s.as_str()) {
            Some("full") => OutputStyleOption::Full,
            Some("basic") => OutputStyleOption::Basic,
            Some("nocolor") => OutputStyleOption::NoColor,
            Some("color") => OutputStyleOption::Color,
            Some("none") => OutputStyleOption::Disabled,
            _ => {
                if options
                    .command_output_policies
                    .contains(&CommandOutputPolicy::Inherit)
                    || !io::stdout().is_terminal()
                {
                    OutputStyleOption::Basic
                } else if env::var_os("TERM")
                    .map(|t| t == "unknown" || t == "dumb")
                    .unwrap_or(!cfg!(target_os = "windows"))
                    || env::var_os("NO_COLOR")
                        .map(|t| !t.is_empty())
                        .unwrap_or(false)
                {
                    OutputStyleOption::NoColor
                } else {
                    OutputStyleOption::Full
                }
            }
        };

        match options.output_style {
            OutputStyleOption::Basic | OutputStyleOption::NoColor => {
                colored::control::set_override(false)
            }
            OutputStyleOption::Full | OutputStyleOption::Color => {
                colored::control::set_override(true)
            }
            OutputStyleOption::Disabled => {}
        };

        (
            options.sort_order_speed_comparison,
            options.sort_order_exports,
        ) = match matches.get_one::<String>("sort").map(|s| s.as_str()) {
            None | Some("auto") => (SortOrder::MeanTime, SortOrder::Command),
            Some("command") => (SortOrder::Command, SortOrder::Command),
            Some("mean-time") => (SortOrder::MeanTime, SortOrder::MeanTime),
            Some(_) => unreachable!("Unknown sort order"),
        };

        options.executor_kind = if matches.get_flag("no-shell") {
            ExecutorKind::Raw
        } else {
            match (
                matches.get_flag("debug-mode"),
                matches.get_one::<String>("shell"),
            ) {
                (false, Some(shell)) if shell == "default" => ExecutorKind::Shell(Shell::default()),
                (false, Some(shell)) if shell == "none" => ExecutorKind::Raw,
                (false, Some(shell)) => ExecutorKind::Shell(Shell::parse_from_str(shell)?),
                (false, None) => ExecutorKind::Shell(Shell::default()),
                (true, Some(shell)) => ExecutorKind::Mock(Some(shell.into())),
                (true, None) => ExecutorKind::Mock(None),
            }
        };

        if let Some(mode) = matches.get_one::<String>("ignore-failure") {
            options.command_failure_action = match mode.as_str() {
                "all-non-zero" | "" => CmdFailureAction::IgnoreAllFailures,
                codes => {
                    let exit_codes: Result<Vec<i32>, _> = codes
                        .split(',')
                        .map(|s| {
                            s.trim()
                                .parse::<i32>()
                                .map_err(|e| OptionsError::IntParsingError("ignore-failure", e))
                        })
                        .collect();
                    CmdFailureAction::IgnoreSpecificFailures(exit_codes?)
                }
            };
        }

        options.time_unit = match matches.get_one::<String>("time-unit").map(|s| s.as_str()) {
            Some("microsecond") => Some(Unit::MicroSecond),
            Some("millisecond") => Some(Unit::MilliSecond),
            Some("second") => Some(Unit::Second),
            _ => None,
        };

        if let Some(time) = matches.get_one::<String>("min-benchmarking-time") {
            options.min_benchmarking_time = time
                .parse::<f64>()
                .map_err(|e| OptionsError::FloatParsingError("min-benchmarking-time", e))?;
        }

        options.command_input_policy = if let Some(path_str) = matches.get_one::<String>("input") {
            if path_str == "null" {
                CommandInputPolicy::Null
            } else {
                let path = PathBuf::from(path_str);
                if !path.exists() {
                    return Err(OptionsError::StdinDataFileDoesNotExist(
                        path_str.to_string(),
                    ));
                }
                CommandInputPolicy::File(path)
            }
        } else {
            CommandInputPolicy::Null
        };

        options.measure_energy = matches.get_flag("energy");
        options.deep_stats = matches.get_flag("deep-stats");
        options.filter_failed = matches.get_flag("filter-failed");
        options.suppress_outlier_warnings = matches.get_flag("suppress-outlier-warnings");
        options.omit_failed_runs = matches.get_flag("omit-failed-runs");
        options.show_resource_usage = matches.get_flag("resource-usage");
        options.discard_outliers = matches
            .get_one::<String>("discard-outliers")
            .map(|value| match value.as_str() {
                "default" => Ok(crate::outlier_detection::OUTLIER_THRESHOLD),
                other => other
                    .parse::<f64>()
                    .ok()
                    .filter(|z| z.is_finite() && *z > 0.0)
                    .ok_or(OptionsError::InvalidOutlierThreshold(other.to_string())),
            })
            .transpose()?;

        if options.omit_failed_runs
            && options.command_failure_action == CmdFailureAction::RaiseError
        {
            return Err(OptionsError::OmitFailedRunsRequiresIgnoreFailure);
        }

        options.schedule = if matches.get_flag("round-robin") {
            ScheduleMode::RoundRobin
        } else {
            match matches.get_one::<String>("schedule").map(|s| s.as_str()) {
                Some("round-robin") | Some("interleaved") => ScheduleMode::RoundRobin,
                _ => ScheduleMode::Grouped,
            }
        };

        options.allow_setup_with_round_robin = matches.get_flag("allow-setup-with-round-robin");

        Ok(options)
    }

    pub fn validate_against_command_list(&mut self, commands: &Commands) -> Result<()> {
        let has_reference_command = self.reference_command.is_some();
        let num_commands = commands.num_commands(has_reference_command);

        if num_commands == 0 {
            return Ok(());
        }

        if self.schedule == ScheduleMode::RoundRobin && !self.allow_setup_with_round_robin {
            let setup_has_param = self.setup_command.as_ref().is_some_and(|setup| {
                commands.iter().any(|c| {
                    c.get_parameters()
                        .iter()
                        .any(|(n, _)| setup.contains(&format!("{{{n}}}")))
                })
            });
            let cleanup_has_param = self.cleanup_command.as_ref().is_some_and(|cleanup| {
                commands.iter().any(|c| {
                    c.get_parameters()
                        .iter()
                        .any(|(n, _)| cleanup.contains(&format!("{{{n}}}")))
                })
            });
            if setup_has_param || cleanup_has_param {
                bail!(
                    "The '--setup' and/or '--cleanup' options differ between benchmarks (due to parameter substitution) \
                     and cannot be combined with '--schedule round-robin'. \
                     Setup runs once per benchmark batch, but round-robin interleaves runs across benchmarks. \
                     Use '--prepare' for per-run state, or use the default grouped schedule."
                );
            }
        }

        if let Some(preparation_command) = &self.preparation_command {
            ensure!(
                preparation_command.len() <= 1 || num_commands == preparation_command.len(),
                "The '--prepare' option has to be provided just once or N times, where N={num_commands} is the \
                 number of benchmark commands (including a potential reference)."
            );
        }

        if let Some(conclusion_command) = &self.conclusion_command {
            ensure!(
                conclusion_command.len() <= 1 || num_commands == conclusion_command.len(),
                "The '--conclude' option has to be provided just once or N times, where N={num_commands} is the \
                 number of benchmark commands (including a potential reference)."
            );
        }

        if self.command_output_policies.len() == 1 {
            self.command_output_policies =
                vec![self.command_output_policies[0].clone(); num_commands];
        } else {
            ensure!(
                self.command_output_policies.len() == num_commands,
                "The '--output' option has to be provided just once or N times, where N={num_commands} is the \
                 number of benchmark commands (including a potential reference)."
            );
        }

        Ok(())
    }
}

#[test]
fn test_default_shell() {
    let shell = Shell::default();

    let s = format!("{shell}");
    assert_eq!(&s, DEFAULT_SHELL);

    let cmd = shell.command();
    assert_eq!(cmd.get_program(), DEFAULT_SHELL);
}

#[test]
fn test_can_parse_shell_command_line_from_str() {
    let shell = Shell::parse_from_str("shell -x 'aaa bbb'").unwrap();

    let s = format!("{shell}");
    assert_eq!(&s, "shell -x 'aaa bbb'");

    let cmd = shell.command();
    assert_eq!(cmd.get_program().to_string_lossy(), "shell");
    assert_eq!(
        cmd.get_args()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>(),
        vec!["-x", "aaa bbb"]
    );

    // Error cases
    assert!(matches!(
        Shell::parse_from_str("shell 'foo").unwrap_err(),
        OptionsError::ShellParseError(_)
    ));

    assert!(matches!(
        Shell::parse_from_str("").unwrap_err(),
        OptionsError::EmptyShell
    ));

    assert!(matches!(
        Shell::parse_from_str("''").unwrap_err(),
        OptionsError::EmptyShell
    ));
}

#[test]
fn test_suppress_outlier_warnings_option() {
    let matches = crate::cli::build_command().get_matches_from(vec![
        "joulex",
        "--suppress-outlier-warnings",
        "echo test",
    ]);
    let options = Options::from_cli_arguments(&matches).unwrap();
    assert!(options.suppress_outlier_warnings);

    let matches_default = crate::cli::build_command().get_matches_from(vec!["joulex", "echo test"]);
    let options_default = Options::from_cli_arguments(&matches_default).unwrap();
    assert!(!options_default.suppress_outlier_warnings);
}

#[test]
fn test_schedule_options() {
    let matches = crate::cli::build_command().get_matches_from(vec![
        "joulex",
        "--schedule=round-robin",
        "echo test",
    ]);
    let options = Options::from_cli_arguments(&matches).unwrap();
    assert_eq!(options.schedule, ScheduleMode::RoundRobin);

    let matches_interleaved = crate::cli::build_command().get_matches_from(vec![
        "joulex",
        "--schedule=interleaved",
        "echo test",
    ]);
    let options_interleaved = Options::from_cli_arguments(&matches_interleaved).unwrap();
    assert_eq!(options_interleaved.schedule, ScheduleMode::RoundRobin);

    assert!(crate::cli::build_command()
        .try_get_matches_from(vec!["joulex", "--schedule=sequential", "echo test"])
        .is_err());

    let matches_flag =
        crate::cli::build_command().get_matches_from(vec!["joulex", "--round-robin", "echo test"]);
    let options_flag = Options::from_cli_arguments(&matches_flag).unwrap();
    assert_eq!(options_flag.schedule, ScheduleMode::RoundRobin);

    let matches_default = crate::cli::build_command().get_matches_from(vec!["joulex", "echo test"]);
    let options_default = Options::from_cli_arguments(&matches_default).unwrap();
    assert_eq!(options_default.schedule, ScheduleMode::Grouped);
}
