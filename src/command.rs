use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

#[cfg(test)]
use crate::parameter::range_step::Numeric;
use crate::parameter::tokenize::tokenize;
use crate::parameter::ParameterValue;
use crate::{
    error::{OptionsError, ParameterScanError},
    parameter::{range_step::RangeStep, ParameterNameAndValue},
};

use clap::ArgMatches;

use anyhow::{anyhow, bail, Context, Result};
use rust_decimal::Decimal;

/// A command that should be benchmarked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command<'a> {
    /// The command name (without parameter substitution)
    name: Option<&'a str>,

    /// The command that should be executed (without parameter substitution)
    expression: &'a str,

    /// Zero or more parameter values.
    parameters: Vec<ParameterNameAndValue<'a>>,
}

impl<'a> Command<'a> {
    pub fn new(name: Option<&'a str>, expression: &'a str) -> Command<'a> {
        Command {
            name,
            expression,
            parameters: Vec::new(),
        }
    }

    pub fn new_parametrized(
        name: Option<&'a str>,
        expression: &'a str,
        parameters: impl IntoIterator<Item = ParameterNameAndValue<'a>>,
    ) -> Command<'a> {
        Command {
            name,
            expression,
            parameters: parameters.into_iter().collect(),
        }
    }

    pub fn get_name(&self) -> String {
        self.name.map_or_else(
            || self.get_command_line(),
            |name| self.replace_parameters_in(name),
        )
    }

    pub fn get_name_with_unused_parameters(&self) -> String {
        let parameters = self
            .get_unused_parameters()
            .fold(String::new(), |output, (parameter, value)| {
                output + &format!("{parameter} = {value}, ")
            });
        let parameters = parameters.trim_end_matches(", ");
        let parameters = if parameters.is_empty() {
            "".into()
        } else {
            format!(" ({parameters})")
        };

        format!("{}{}", self.get_name(), parameters)
    }

    pub fn get_command_line(&self) -> String {
        self.replace_parameters_in(self.expression)
    }

    pub fn get_command(&self) -> Result<std::process::Command> {
        let command_line = self.get_command_line();
        let mut tokens = shell_words::split(&command_line)
            .with_context(|| format!("Failed to parse command '{command_line}'"))?
            .into_iter();

        if let Some(program_name) = tokens.next() {
            let mut command_builder = std::process::Command::new(program_name);
            command_builder.args(tokens);
            Ok(command_builder)
        } else {
            bail!("Can not execute empty command")
        }
    }

    pub fn get_parameters(&self) -> &[(&'a str, ParameterValue)] {
        &self.parameters
    }

    pub fn get_unused_parameters(&self) -> impl Iterator<Item = &(&'a str, ParameterValue)> {
        self.parameters
            .iter()
            .filter(move |(parameter, _)| !self.expression.contains(&format!("{{{parameter}}}")))
    }

    fn replace_parameters_in(&self, original: &str) -> String {
        let mut result = String::new();
        let mut replacements = BTreeMap::<String, String>::new();
        for (param_name, param_value) in &self.parameters {
            replacements.insert(format!("{{{param_name}}}"), param_value.to_string());
        }
        let mut remaining = original;
        // Manually replace consecutive occurrences to avoid double-replacing: e.g.,
        //
        //     hyperfine -L foo 'a,{bar}' -L bar 'baz,quux' 'echo {foo} {bar}'
        //
        // should not ever run 'echo baz baz'. See `test_get_command_line_nonoverlapping`.
        'outer: while let Some(head) = remaining.chars().next() {
            for (k, v) in &replacements {
                if remaining.starts_with(k.as_str()) {
                    result.push_str(v);
                    remaining = &remaining[k.len()..];
                    continue 'outer;
                }
            }
            result.push(head);
            remaining = &remaining[head.len_utf8()..];
        }
        result
    }
}

/// A collection of commands that should be benchmarked
#[derive(Debug)]
pub struct Commands<'a>(Vec<Command<'a>>);

impl<'a> Commands<'a> {
    pub fn from_cli_arguments(matches: &'a ArgMatches) -> Result<Commands<'a>> {
        let command_names = matches.get_many::<String>("command-name");
        let command_strings = matches
            .get_many::<String>("command")
            .unwrap_or_default()
            .map(|v| v.as_str())
            .collect::<Vec<_>>();

        let has_parameters = matches.get_many::<String>("parameter-scan").is_some()
            || matches.get_many::<String>("parameter-list").is_some()
            || matches.get_many::<String>("parameter-file").is_some();

        if has_parameters {
            let command_names = command_names.map_or(vec![], |names| {
                names.map(|v| v.as_str()).collect::<Vec<_>>()
            });

            let mut param_names_and_values: Vec<(&str, Vec<ParameterValue>)> = Vec::new();

            if let Some(args) = matches.get_many::<String>("parameter-scan") {
                let args: Vec<_> = args.map(|v| v.as_str()).collect::<Vec<_>>();
                let step_size = matches
                    .get_one::<String>("parameter-step-size")
                    .map(|s| s.as_str());

                let (chunks, _) = args.as_chunks::<3>();
                if step_size.is_some() && chunks.len() > 1 {
                    bail!("The '--parameter-step-size' ('-D') option cannot be used when multiple '--parameter-scan' ('-P') options are specified");
                }

                for &[name, min, max] in chunks {
                    let values = Self::parse_parameter_scan(min, max, step_size)?;
                    param_names_and_values.push((name, values));
                }
            }

            if let Some(args) = matches.get_many::<String>("parameter-list") {
                let args: Vec<_> = args.map(|v| v.as_str()).collect::<Vec<_>>();
                for &[name, list_str] in args.as_chunks::<2>().0 {
                    let values = tokenize(list_str)
                        .into_iter()
                        .map(ParameterValue::Text)
                        .collect();
                    param_names_and_values.push((name, values));
                }
            }

            if let Some(args) = matches.get_many::<String>("parameter-file") {
                let args: Vec<_> = args.map(|v| v.as_str()).collect::<Vec<_>>();
                for &[name, file_path] in args.as_chunks::<2>().0 {
                    let content = std::fs::read_to_string(file_path)
                        .with_context(|| format!("Could not read parameter file '{file_path}'"))?;
                    let lines: Vec<ParameterValue> = content
                        .lines()
                        .map(|l| l.trim_end_matches('\r').to_string())
                        .filter(|l| !l.is_empty())
                        .map(ParameterValue::Text)
                        .collect();
                    if lines.is_empty() {
                        bail!("Parameter file '{file_path}' contains no values");
                    }
                    param_names_and_values.push((name, lines));
                }
            }

            {
                let duplicates =
                    Self::find_duplicates(param_names_and_values.iter().map(|(name, _)| *name));
                if !duplicates.is_empty() {
                    bail!("Duplicate parameter names: {}", duplicates.join(", "));
                }
            }

            let dimensions: Vec<usize> = std::iter::once(command_strings.len())
                .chain(
                    param_names_and_values
                        .iter()
                        .map(|(_, values)| values.len()),
                )
                .collect();

            let max_benchmarks = matches
                .get_one::<usize>("max-benchmarks")
                .copied()
                .unwrap_or(100_000);

            let param_space_size = dimensions
                .iter()
                .try_fold(1usize, |acc, &len| acc.checked_mul(len))
                .filter(|&n| n <= max_benchmarks)
                .ok_or_else(|| {
                    let breakdown = std::iter::once(format!("commands: {}", command_strings.len()))
                        .chain(
                            param_names_and_values
                                .iter()
                                .map(|(name, values)| format!("{name}: {}", values.len())),
                        )
                        .collect::<Vec<_>>()
                        .join(" × ");
                    anyhow!(
                        "The parameter combinations would create more than {max_benchmarks} benchmarks \
                         ({breakdown}). Reduce the ranges, use --parameter-step-size, or override with --max-benchmarks."
                    )
                })?;

            if param_space_size == 0 {
                return Ok(Self(Vec::new()));
            }

            // `--command-name` should appear exactly once or exactly B times,
            // where B is the total number of benchmarks.
            let command_name_count = command_names.len();
            if command_name_count > 1 && command_name_count != param_space_size {
                return Err(OptionsError::UnexpectedCommandNameCount(
                    command_name_count,
                    param_space_size,
                )
                .into());
            }

            let mut i = 0;
            let mut commands = Vec::with_capacity(param_space_size);
            let mut index = vec![0usize; dimensions.len()];
            'outer: loop {
                let name = command_names
                    .get(i)
                    .or_else(|| command_names.first())
                    .copied();
                i += 1;

                let (command_index, params_indices) = index.split_first().unwrap();
                let parameters: Vec<_> = param_names_and_values
                    .iter()
                    .zip(params_indices)
                    .map(|((name, values), i)| (*name, values[*i].clone()))
                    .collect();
                commands.push(Command::new_parametrized(
                    name,
                    command_strings[*command_index],
                    parameters,
                ));

                // Increment index, exiting loop on overflow.
                for (i, n) in index.iter_mut().zip(dimensions.iter()) {
                    *i += 1;
                    if *i < *n {
                        continue 'outer;
                    } else {
                        *i = 0;
                    }
                }
                break 'outer;
            }

            Ok(Self(commands))
        } else {
            let command_names = command_names.map_or(vec![], |names| {
                names.map(|v| v.as_str()).collect::<Vec<_>>()
            });
            if command_names.len() > command_strings.len() {
                return Err(OptionsError::TooManyCommandNames(command_strings.len()).into());
            }

            let mut commands = Vec::with_capacity(command_strings.len());
            for (i, s) in command_strings.iter().enumerate() {
                commands.push(Command::new(command_names.get(i).copied(), s));
            }
            Ok(Self(commands))
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Command<'a>> {
        self.0.iter()
    }

    pub fn num_commands(&self, has_reference_command: bool) -> usize {
        self.0.len() + if has_reference_command { 1 } else { 0 }
    }

    /// Finds all the strings that appear multiple times in the input iterator, returning them in
    /// sorted order. If no string appears more than once, the result is an empty vector.
    fn find_duplicates<'b, I: IntoIterator<Item = &'b str>>(i: I) -> Vec<&'b str> {
        let mut counts = BTreeMap::<&'b str, usize>::new();
        for s in i {
            *counts.entry(s).or_default() += 1;
        }
        counts
            .into_iter()
            .filter_map(|(k, n)| if n > 1 { Some(k) } else { None })
            .collect()
    }

    #[cfg(test)]
    fn build_parameter_scan_commands<'b, T: Numeric>(
        param_name: &'b str,
        param_min: T,
        param_max: T,
        step: T,
        command_names: Vec<&'b str>,
        command_strings: Vec<&'b str>,
    ) -> Result<Vec<Command<'b>>, ParameterScanError> {
        let param_range = RangeStep::new(param_min, param_max, step)?;
        let command_name_count = command_names.len();

        let mut i = 0;
        let mut commands = vec![];
        for value in param_range {
            for cmd in &command_strings {
                let name = command_names
                    .get(i)
                    .or_else(|| command_names.first())
                    .copied();
                commands.push(Command::new_parametrized(
                    name,
                    cmd,
                    vec![(param_name, ParameterValue::Numeric(value.into()))],
                ));
                i += 1;
            }
        }

        // `--command-name` should appear exactly once or exactly B times,
        // where B is the total number of benchmarks.
        let command_count = commands.len();
        if command_name_count > 1 && command_name_count != command_count {
            return Err(ParameterScanError::UnexpectedCommandNameCount(
                command_name_count,
                command_count,
            ));
        }

        Ok(commands)
    }

    fn parse_parameter_scan(
        param_min: &str,
        param_max: &str,
        step: Option<&str>,
    ) -> Result<Vec<ParameterValue>, ParameterScanError> {
        // attempt to parse as integers
        if let (Ok(param_min), Ok(param_max), Ok(step)) = (
            param_min.parse::<i32>(),
            param_max.parse::<i32>(),
            step.unwrap_or("1").parse::<i32>(),
        ) {
            let param_range = RangeStep::new(param_min, param_max, step)?;
            return Ok(param_range
                .map(|v| ParameterValue::Numeric(v.into()))
                .collect());
        }

        // try parsing them as decimals
        let param_min = Decimal::from_str(param_min)?;
        let param_max = Decimal::from_str(param_max)?;

        if step.is_none() {
            return Err(ParameterScanError::StepRequired);
        }

        let step = Decimal::from_str(step.unwrap())?;
        let param_range = RangeStep::new(param_min, param_max, step)?;
        Ok(param_range
            .map(|v| ParameterValue::Numeric(v.into()))
            .collect())
    }
}

#[test]
fn test_get_command_line_nonoverlapping() {
    let cmd = Command::new_parametrized(
        None,
        "echo {foo} {bar}",
        vec![
            ("foo", ParameterValue::Text("{bar} baz".into())),
            ("bar", ParameterValue::Text("quux".into())),
        ],
    );
    assert_eq!(cmd.get_command_line(), "echo {bar} baz quux");
}

#[test]
fn test_get_parameterized_command_name() {
    let cmd = Command::new_parametrized(
        Some("name-{bar}-{foo}"),
        "echo {foo} {bar}",
        vec![
            ("foo", ParameterValue::Text("baz".into())),
            ("bar", ParameterValue::Text("quux".into())),
        ],
    );
    assert_eq!(cmd.get_name(), "name-quux-baz");
}

impl fmt::Display for Command<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get_command_line())
    }
}

#[test]
fn test_build_commands_cross_product() {
    use crate::cli::get_cli_arguments;

    let matches = get_cli_arguments(vec![
        "hyperfine",
        "-L",
        "par1",
        "a,b",
        "-L",
        "par2",
        "z,y",
        "echo {par1} {par2}",
        "printf '%s\n' {par1} {par2}",
    ]);
    let result = Commands::from_cli_arguments(&matches).unwrap().0;

    // Iteration order: command list first, then parameters in listed order (here, "par1" before
    // "par2", which is distinct from their sorted order), with parameter values in listed order.
    let pv = |s: &str| ParameterValue::Text(s.to_string());
    let cmd = |cmd: usize, par1: &str, par2: &str| {
        let expression = ["echo {par1} {par2}", "printf '%s\n' {par1} {par2}"][cmd];
        let params = vec![("par1", pv(par1)), ("par2", pv(par2))];
        Command::new_parametrized(None, expression, params)
    };
    let expected = vec![
        cmd(0, "a", "z"),
        cmd(1, "a", "z"),
        cmd(0, "b", "z"),
        cmd(1, "b", "z"),
        cmd(0, "a", "y"),
        cmd(1, "a", "y"),
        cmd(0, "b", "y"),
        cmd(1, "b", "y"),
    ];
    assert_eq!(result, expected);
}

#[test]
fn test_build_parameter_list_commands() {
    use crate::cli::get_cli_arguments;

    let matches = get_cli_arguments(vec![
        "hyperfine",
        "echo {foo}",
        "--parameter-list",
        "foo",
        "1,2",
        "--command-name",
        "name-{foo}",
    ]);
    let commands = Commands::from_cli_arguments(&matches).unwrap().0;
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].get_name(), "name-1");
    assert_eq!(commands[1].get_name(), "name-2");
    assert_eq!(commands[0].get_command_line(), "echo 1");
    assert_eq!(commands[1].get_command_line(), "echo 2");
}

#[test]
fn test_build_parameter_scan_commands() {
    use crate::cli::get_cli_arguments;
    let matches = get_cli_arguments(vec![
        "hyperfine",
        "echo {val}",
        "--parameter-scan",
        "val",
        "1",
        "2",
        "--parameter-step-size",
        "1",
        "--command-name",
        "name-{val}",
    ]);
    let commands = Commands::from_cli_arguments(&matches).unwrap().0;
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].get_name(), "name-1");
    assert_eq!(commands[1].get_name(), "name-2");
    assert_eq!(commands[0].get_command_line(), "echo 1");
    assert_eq!(commands[1].get_command_line(), "echo 2");
}

#[test]
fn test_build_parameter_scan_commands_named() {
    use crate::cli::get_cli_arguments;
    let matches = get_cli_arguments(vec![
        "hyperfine",
        "echo {val}",
        "sleep {val}",
        "--parameter-scan",
        "val",
        "1",
        "2",
        "--parameter-step-size",
        "1",
        "--command-name",
        "echo-1",
        "--command-name",
        "sleep-1",
        "--command-name",
        "echo-2",
        "--command-name",
        "sleep-2",
    ]);
    let commands = Commands::from_cli_arguments(&matches).unwrap().0;
    assert_eq!(commands.len(), 4);
    assert_eq!(commands[0].get_name(), "echo-1");
    assert_eq!(commands[0].get_command_line(), "echo 1");
    assert_eq!(commands[1].get_name(), "sleep-1");
    assert_eq!(commands[1].get_command_line(), "sleep 1");
    assert_eq!(commands[2].get_name(), "echo-2");
    assert_eq!(commands[2].get_command_line(), "echo 2");
    assert_eq!(commands[3].get_name(), "sleep-2");
    assert_eq!(commands[3].get_command_line(), "sleep 2");
}

#[test]
fn test_parameter_scan_commands_int() {
    let commands = Commands::build_parameter_scan_commands(
        "val",
        1i32,
        7i32,
        3i32,
        vec![],
        vec!["echo {val}"],
    )
    .unwrap();
    assert_eq!(commands.len(), 3);
    assert_eq!(commands[2].get_name(), "echo 7");
    assert_eq!(commands[2].get_command_line(), "echo 7");
}

#[test]
fn test_parameter_scan_commands_decimal() {
    let param_min = Decimal::from_str("0").unwrap();
    let param_max = Decimal::from_str("1").unwrap();
    let step = Decimal::from_str("0.33").unwrap();

    let commands = Commands::build_parameter_scan_commands(
        "val",
        param_min,
        param_max,
        step,
        vec![],
        vec!["echo {val}"],
    )
    .unwrap();
    assert_eq!(commands.len(), 4);
    assert_eq!(commands[3].get_name(), "echo 0.99");
    assert_eq!(commands[3].get_command_line(), "echo 0.99");
}

#[test]
fn test_parameter_scan_commands_names() {
    let commands = Commands::build_parameter_scan_commands(
        "val",
        1i32,
        3i32,
        1i32,
        vec!["name-{val}"],
        vec!["echo {val}"],
    )
    .unwrap();
    assert_eq!(commands.len(), 3);
    let command_names = commands
        .iter()
        .map(|c| c.get_name())
        .collect::<Vec<String>>();
    assert_eq!(command_names, vec!["name-1", "name-2", "name-3"]);
}

#[test]
fn test_get_specified_command_names() {
    let commands = Commands::build_parameter_scan_commands(
        "val",
        1i32,
        3i32,
        1i32,
        vec!["name-a", "name-b", "name-c"],
        vec!["echo {val}"],
    )
    .unwrap();
    assert_eq!(commands.len(), 3);
    let command_names = commands
        .iter()
        .map(|c| c.get_name())
        .collect::<Vec<String>>();
    assert_eq!(command_names, vec!["name-a", "name-b", "name-c"]);
}

#[test]
fn test_different_command_name_count_with_parameters() {
    let result = Commands::build_parameter_scan_commands(
        "val",
        1i32,
        3i32,
        1i32,
        vec!["name-1", "name-2"],
        vec!["echo {val}"],
    );
    assert!(matches!(
        result.unwrap_err(),
        ParameterScanError::UnexpectedCommandNameCount(2, 3)
    ));
}

#[test]
fn test_parameter_file_support() {
    use crate::cli::get_cli_arguments;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, "foo\r\nbar\n\nbaz\n").unwrap();
    let temp_path = temp.path().to_str().unwrap().to_string();

    let matches = get_cli_arguments(vec!["joulex", "-F", "item", &temp_path, "echo {item}"]);
    let commands = Commands::from_cli_arguments(&matches).unwrap();
    assert_eq!(commands.iter().count(), 3);
    let names: Vec<_> = commands.iter().map(|c| c.get_command_line()).collect();
    assert_eq!(names, vec!["echo foo", "echo bar", "echo baz"]);
}

#[test]
fn test_parameter_file_and_list_combined() {
    use crate::cli::get_cli_arguments;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, "1\n2").unwrap();
    let temp_path = temp.path().to_str().unwrap().to_string();

    let matches = get_cli_arguments(vec![
        "joulex",
        "-F",
        "num",
        &temp_path,
        "-L",
        "letter",
        "a,b",
        "echo {num}-{letter}",
    ]);
    let commands = Commands::from_cli_arguments(&matches).unwrap();
    assert_eq!(commands.iter().count(), 4);
}

#[test]
fn test_multiple_parameter_scans() {
    use crate::cli::get_cli_arguments;

    let matches = get_cli_arguments(vec![
        "joulex",
        "-P",
        "a",
        "1",
        "2",
        "-P",
        "b",
        "10",
        "11",
        "echo {a} {b}",
    ]);
    let commands = Commands::from_cli_arguments(&matches).unwrap().0;
    assert_eq!(commands.len(), 4);
    let lines: Vec<_> = commands.iter().map(|c| c.get_command_line()).collect();
    assert_eq!(
        lines,
        vec!["echo 1 10", "echo 2 10", "echo 1 11", "echo 2 11",]
    );
}

#[test]
fn test_parameter_scan_and_list_combined() {
    use crate::cli::get_cli_arguments;

    let matches = get_cli_arguments(vec![
        "joulex",
        "-P",
        "threads",
        "1",
        "2",
        "-L",
        "opt",
        "O1,O2",
        "make -j {threads} {opt}",
    ]);
    let commands = Commands::from_cli_arguments(&matches).unwrap().0;
    assert_eq!(commands.len(), 4);
    let lines: Vec<_> = commands.iter().map(|c| c.get_command_line()).collect();
    assert_eq!(
        lines,
        vec![
            "make -j 1 O1",
            "make -j 2 O1",
            "make -j 1 O2",
            "make -j 2 O2",
        ]
    );
}

#[test]
fn test_parameter_scan_and_file_combined() {
    use crate::cli::get_cli_arguments;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, "alpha\nbeta").unwrap();
    let temp_path = temp.path().to_str().unwrap().to_string();

    let matches = get_cli_arguments(vec![
        "joulex",
        "-P",
        "iter",
        "1",
        "2",
        "-F",
        "target",
        &temp_path,
        "run {iter} {target}",
    ]);
    let commands = Commands::from_cli_arguments(&matches).unwrap().0;
    assert_eq!(commands.len(), 4);
    let lines: Vec<_> = commands.iter().map(|c| c.get_command_line()).collect();
    assert_eq!(
        lines,
        vec!["run 1 alpha", "run 2 alpha", "run 1 beta", "run 2 beta",]
    );
}

#[test]
fn test_multiple_parameter_scans_with_step_size_fails() {
    use crate::cli::get_cli_arguments;

    let matches = get_cli_arguments(vec![
        "joulex",
        "-P",
        "a",
        "1",
        "5",
        "-P",
        "b",
        "1",
        "5",
        "-D",
        "2",
        "echo {a} {b}",
    ]);
    let err = Commands::from_cli_arguments(&matches).unwrap_err();
    assert!(err
        .to_string()
        .contains("The '--parameter-step-size' ('-D') option cannot be used when multiple '--parameter-scan' ('-P') options are specified"));
}

#[test]
fn test_single_parameter_scan_with_step_size_succeeds() {
    use crate::cli::get_cli_arguments;

    let matches = get_cli_arguments(vec!["joulex", "-P", "a", "1", "5", "-D", "2", "echo {a}"]);
    let commands = Commands::from_cli_arguments(&matches).unwrap().0;
    assert_eq!(commands.len(), 3);
    let lines: Vec<_> = commands.iter().map(|c| c.get_command_line()).collect();
    assert_eq!(lines, vec!["echo 1", "echo 3", "echo 5"]);
}

#[test]
fn test_parameter_combinations_exceeding_default_limit_fails() {
    use crate::cli::get_cli_arguments;

    let matches = get_cli_arguments(vec![
        "joulex",
        "-P",
        "a",
        "1",
        "20000",
        "-P",
        "b",
        "1",
        "20000",
        "echo {a} {b}",
    ]);
    let err = Commands::from_cli_arguments(&matches).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("The parameter combinations would create more than 100000 benchmarks"));
    assert!(msg.contains("commands: 1 × a: 20000 × b: 20000"));
}

#[test]
fn test_parameter_combinations_max_benchmarks_override() {
    use crate::cli::get_cli_arguments;

    // 10 x 10 = 100 combinations. With --max-benchmarks 50 it should fail:
    let matches_fail = get_cli_arguments(vec![
        "joulex",
        "-P",
        "a",
        "1",
        "10",
        "-P",
        "b",
        "1",
        "10",
        "--max-benchmarks",
        "50",
        "echo {a} {b}",
    ]);
    let err = Commands::from_cli_arguments(&matches_fail).unwrap_err();
    assert!(err
        .to_string()
        .contains("The parameter combinations would create more than 50 benchmarks"));

    // With --max-benchmarks 200 it should succeed:
    let matches_ok = get_cli_arguments(vec![
        "joulex",
        "-P",
        "a",
        "1",
        "10",
        "-P",
        "b",
        "1",
        "10",
        "--max-benchmarks",
        "200",
        "echo {a} {b}",
    ]);
    let commands = Commands::from_cli_arguments(&matches_ok).unwrap().0;
    assert_eq!(commands.len(), 100);
}
