use std::borrow::Cow;
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

/// Name of the built-in placeholder `{iteration}`, which expands to the same
/// value as the `JOULEX_ITERATION` environment variable (e.g. `0`, `1`, or
/// `warmup-0`). Unlike the environment variable, it also works with
/// `--shell=none`.
pub const ITERATION_PLACEHOLDER: &str = "iteration";

/// How the values of per-run parameters are chosen for each run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerRunMode {
    /// Go through all combinations in order (`--aggregate-parameter-runs`)
    Cycle,
    /// Draw a combination at random for every run (`--parameter-sample`). The
    /// draw depends only on the seed and the run index, so every command sees
    /// the same values in the same order (a paired comparison).
    Random { seed: u64 },
}

/// Parameters that are substituted per run instead of per benchmark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerRunParameters<'a> {
    vars: Vec<(&'a str, Vec<ParameterValue>)>,
    mode: PerRunMode,
}

impl<'a> PerRunParameters<'a> {
    pub fn new(vars: Vec<(&'a str, Vec<ParameterValue>)>, mode: PerRunMode) -> Self {
        PerRunParameters { vars, mode }
    }

    /// Number of value combinations
    pub fn combinations(&self) -> usize {
        self.vars.iter().map(|(_, values)| values.len()).product()
    }

    /// The values for one run; none for non-benchmark runs (setup, cleanup).
    fn values_for(
        &self,
        iteration: &crate::benchmark::executor::BenchmarkIteration,
    ) -> Vec<ParameterNameAndValue<'a>> {
        use crate::benchmark::executor::BenchmarkIteration;
        use rand::{rngs::StdRng, Rng, SeedableRng};

        let (index, warmup) = match *iteration {
            BenchmarkIteration::Benchmark(i) => (i, false),
            BenchmarkIteration::Warmup(i) => (i, true),
            BenchmarkIteration::NonBenchmarkRun => return Vec::new(),
        };
        match self.mode {
            PerRunMode::Cycle => {
                // Mixed-radix digits of the run index; the last variable
                // changes fastest
                let mut rest = (index % self.combinations().max(1) as u64) as usize;
                let mut values: Vec<_> = self
                    .vars
                    .iter()
                    .rev()
                    .map(|(name, values)| {
                        let value = values[rest % values.len()].clone();
                        rest /= values.len();
                        (*name, value)
                    })
                    .collect();
                values.reverse();
                values
            }
            PerRunMode::Random { seed } => {
                let stream = index.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ u64::from(warmup) << 63;
                let mut rng = StdRng::seed_from_u64(seed ^ stream);
                self.vars
                    .iter()
                    .map(|(name, values)| (*name, values[rng.gen_range(0..values.len())].clone()))
                    .collect()
            }
        }
    }

    /// Shown after the command name, e.g. "(sampled from 24 values)"
    fn description(&self) -> String {
        match self.mode {
            PerRunMode::Cycle => {
                format!("aggregated over {} parameter values", self.combinations())
            }
            PerRunMode::Random { .. } => format!("sampled from {} values", self.combinations()),
        }
    }
}

/// A command that should be benchmarked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command<'a> {
    /// The command name (without parameter substitution)
    name: Option<&'a str>,

    /// The command that should be executed (without parameter substitution).
    /// For a command given as an argument vector, this is its shell-quoted
    /// form, used for display and for finding the parameters it uses.
    expression: Cow<'a, str>,

    /// The exact argument vector, for a command given after `--`. It is
    /// executed as is (with parameters substituted per argument) instead of
    /// re-splitting `expression`.
    argv: Option<Vec<&'a str>>,

    /// Zero or more parameter values.
    parameters: Vec<ParameterNameAndValue<'a>>,

    /// Parameters substituted per run (`--parameter-sample`,
    /// `--aggregate-parameter-runs`)
    per_run: Vec<PerRunParameters<'a>>,
}

impl<'a> Command<'a> {
    pub fn new(name: Option<&'a str>, expression: &'a str) -> Command<'a> {
        Self::new_parametrized(name, expression, Vec::new())
    }

    pub fn new_parametrized(
        name: Option<&'a str>,
        expression: &'a str,
        parameters: impl IntoIterator<Item = ParameterNameAndValue<'a>>,
    ) -> Command<'a> {
        Command {
            name,
            expression: Cow::Borrowed(expression),
            argv: None,
            parameters: parameters.into_iter().collect(),
            per_run: Vec::new(),
        }
    }

    /// A command given as an exact argument vector (after `--`).
    pub fn from_argv(
        name: Option<&'a str>,
        argv: Vec<&'a str>,
        parameters: impl IntoIterator<Item = ParameterNameAndValue<'a>>,
    ) -> Command<'a> {
        Command {
            name,
            expression: Cow::Owned(shell_words::join(&argv)),
            argv: Some(argv),
            parameters: parameters.into_iter().collect(),
            per_run: Vec::new(),
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
        let per_run: String = self
            .per_run
            .iter()
            .map(|p| format!(" ({})", p.description()))
            .collect();

        format!("{}{}{}", self.get_name(), parameters, per_run)
    }

    /// Use the same per-run parameters as `other` (for --prepare/--conclude,
    /// so that they see the values of the run they belong to).
    pub fn with_per_run_parameters_of(mut self, other: &Command<'a>) -> Self {
        self.per_run = other.per_run.clone();
        self
    }

    /// With `--aggregate-parameter-runs`, the number of runs must be a
    /// multiple of this, so that every value is used equally often.
    pub fn runs_multiple(&self) -> u64 {
        self.per_run
            .iter()
            .filter(|p| p.mode == PerRunMode::Cycle)
            .map(|p| p.combinations() as u64)
            .product::<u64>()
            .max(1)
    }

    /// The per-run parameter values used in `iteration`, as (name, value);
    /// empty without per-run parameters.
    pub fn per_run_values(
        &self,
        iteration: &crate::benchmark::executor::BenchmarkIteration,
    ) -> Vec<(String, String)> {
        self.per_run
            .iter()
            .flat_map(|p| p.values_for(iteration))
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    /// This command as it runs in `iteration`: `{iteration}` and the per-run
    /// parameters are substituted.
    pub fn for_iteration(
        &self,
        iteration: &crate::benchmark::executor::BenchmarkIteration,
    ) -> std::borrow::Cow<'_, Command<'a>> {
        let command = self.with_iteration(iteration.to_env_var_value());
        if self.per_run.is_empty() {
            return command;
        }
        let mut command = command.into_owned();
        for per_run in &self.per_run {
            command.parameters.extend(per_run.values_for(iteration));
        }
        std::borrow::Cow::Owned(command)
    }

    /// Return a copy of this command in which `{iteration}` expands to `value`.
    /// Without a value (setup/cleanup and other non-benchmark runs), or when
    /// the placeholder isn't used, the command is returned unchanged.
    pub fn with_iteration(&self, value: Option<String>) -> std::borrow::Cow<'_, Command<'a>> {
        let placeholder = format!("{{{ITERATION_PLACEHOLDER}}}");
        match value {
            Some(value) if self.expression.contains(&placeholder) => {
                let mut command = self.clone();
                command
                    .parameters
                    .push((ITERATION_PLACEHOLDER, ParameterValue::Text(value)));
                std::borrow::Cow::Owned(command)
            }
            _ => std::borrow::Cow::Borrowed(self),
        }
    }

    pub fn get_command_line(&self) -> String {
        match &self.argv {
            Some(argv) => shell_words::join(self.get_argv(argv)),
            None => self.replace_parameters_in(&self.expression),
        }
    }

    fn get_argv(&self, argv: &[&str]) -> Vec<String> {
        argv.iter()
            .map(|arg| self.replace_parameters_in(arg))
            .collect()
    }

    pub fn get_command(&self) -> Result<std::process::Command> {
        let mut tokens = match &self.argv {
            Some(argv) => self.get_argv(argv),
            None => {
                let command_line = self.get_command_line();
                // On Windows, split like the C runtime does, so that paths
                // such as `C:\tools\app.exe` keep their backslashes.
                #[cfg(windows)]
                let tokens = split_windows_command_line(&command_line);
                #[cfg(not(windows))]
                let tokens = shell_words::split(&command_line)
                    .with_context(|| format!("Failed to parse command '{command_line}'"))?;
                tokens
            }
        }
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

/// Split a command line into arguments with the rules of the Microsoft C
/// runtime (`CommandLineToArgvW`), which Windows programs use to parse their
/// own command line:
/// - arguments are separated by spaces or tabs, unless inside double quotes;
/// - `2n` backslashes followed by `"` give `n` backslashes and toggle quoting,
///   `2n+1` backslashes followed by `"` give `n` backslashes and a literal `"`;
/// - backslashes that aren't followed by `"` are literal (`C:\tools\app.exe`);
/// - inside quotes, `""` is a literal `"`;
/// - in the program name (the first argument) backslashes are always literal.
#[cfg(any(windows, test))]
fn split_windows_command_line(command_line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut chars = command_line
        .trim_start_matches([' ', '\t'])
        .chars()
        .peekable();

    // The program name: quotes toggle, nothing is escaped.
    if chars.peek().is_some() {
        let mut program = String::new();
        let mut in_quotes = false;
        while let Some(c) = chars.next_if(|&c| in_quotes || !matches!(c, ' ' | '\t')) {
            if c == '"' {
                in_quotes = !in_quotes;
            } else {
                program.push(c);
            }
        }
        args.push(program);
    }

    let mut current = String::new();
    let mut in_arg = false;
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' if !in_quotes => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            '\\' => {
                in_arg = true;
                let mut backslashes = 1;
                while chars.next_if_eq(&'\\').is_some() {
                    backslashes += 1;
                }
                if chars.peek() == Some(&'"') {
                    current.extend(std::iter::repeat_n('\\', backslashes / 2));
                    if backslashes % 2 == 1 {
                        current.push('"');
                        chars.next();
                    }
                } else {
                    current.extend(std::iter::repeat_n('\\', backslashes));
                }
            }
            '"' => {
                in_arg = true;
                if in_quotes && chars.next_if_eq(&'"').is_some() {
                    current.push('"');
                } else {
                    in_quotes = !in_quotes;
                }
            }
            c => {
                in_arg = true;
                current.push(c);
            }
        }
    }
    if in_arg {
        args.push(current);
    }
    args
}

/// A collection of commands that should be benchmarked
#[derive(Debug)]
pub struct Commands<'a>(Vec<Command<'a>>);

impl<'a> Commands<'a> {
    pub fn from_cli_arguments(matches: &'a ArgMatches) -> Result<Commands<'a>> {
        let has_parameters = ["parameter-scan", "parameter-list", "parameter-file"]
            .iter()
            .any(|arg| matches.get_many::<String>(arg).is_some());
        if matches.get_flag("aggregate-parameter-runs") && !has_parameters {
            bail!(
                "'--aggregate-parameter-runs' needs parameters to aggregate over \
                 ('-L', '-P' or '--parameter-file')"
            );
        }

        let mut commands = Self::expand_parameters(matches)?;

        if let Some(args) = matches.get_many::<String>("parameter-sample") {
            let args: Vec<&str> = args.map(String::as_str).collect();
            let mut vars = Vec::new();
            for &[name, list] in args.as_chunks::<2>().0 {
                let values: Vec<ParameterValue> = tokenize(list)
                    .into_iter()
                    .map(ParameterValue::Text)
                    .collect();
                if values.is_empty() {
                    bail!("'--parameter-sample {name}' needs at least one value");
                }
                vars.push((name, values));
            }

            // Names of all other parameters (the first value of each option)
            let other_names = [
                ("parameter-scan", 3),
                ("parameter-list", 2),
                ("parameter-file", 2),
            ]
            .into_iter()
            .filter_map(|(arg, arity)| {
                matches
                    .get_many::<String>(arg)
                    .map(move |values| values.step_by(arity).map(String::as_str))
            })
            .flatten();
            let all_names: Vec<&str> = vars
                .iter()
                .map(|(name, _)| *name)
                .chain(other_names)
                .collect();
            if all_names.contains(&ITERATION_PLACEHOLDER) {
                bail!("The parameter name '{ITERATION_PLACEHOLDER}' is reserved");
            }
            let duplicates = Self::find_duplicates(all_names);
            if !duplicates.is_empty() {
                bail!("Duplicate parameter names: {}", duplicates.join(", "));
            }

            let seed = matches.get_one::<u64>("seed").copied().unwrap_or(0);
            let per_run = PerRunParameters::new(vars, PerRunMode::Random { seed });
            for command in &mut commands.0 {
                command.per_run.push(per_run.clone());
            }
        }

        // --setup/--cleanup run once per benchmark, not per run: they can't
        // use a per-run value (it would be passed on literally as '{name}')
        let per_run_names: Vec<&str> = commands
            .0
            .iter()
            .flat_map(|c| c.per_run.iter())
            .flat_map(|p| p.vars.iter().map(|(name, _)| *name))
            .collect();
        for option in ["setup", "cleanup"] {
            for template in matches.get_many::<String>(option).into_iter().flatten() {
                if let Some(name) = per_run_names
                    .iter()
                    .find(|name| template.contains(&format!("{{{name}}}")))
                {
                    bail!(
                        "'--{option}' runs once per benchmark, so it cannot use the per-run \
                         parameter '{{{name}}}' ('--parameter-sample' / \
                         '--aggregate-parameter-runs'). Use '--prepare' / '--conclude', which \
                         run before / after every run with its values."
                    );
                }
            }
        }

        Ok(commands)
    }

    fn expand_parameters(matches: &'a ArgMatches) -> Result<Commands<'a>> {
        let command_names = matches.get_many::<String>("command-name");
        let argv = matches
            .get_many::<String>("argv")
            .map(|args| args.map(String::as_str).collect::<Vec<_>>());
        // A command after `--` takes the place of the positional commands
        // (they conflict at the CLI level).
        let command_strings = match &argv {
            Some(_) => vec![""],
            None => matches
                .get_many::<String>("command")
                .unwrap_or_default()
                .map(|v| v.as_str())
                .collect::<Vec<_>>(),
        };
        let make_command = |name, expression, parameters| match &argv {
            Some(argv) => Command::from_argv(name, argv.clone(), parameters),
            None => Command::new_parametrized(name, expression, parameters),
        };

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

            if param_names_and_values
                .iter()
                .any(|(name, _)| *name == ITERATION_PLACEHOLDER)
            {
                bail!(
                    "The parameter name '{ITERATION_PLACEHOLDER}' is reserved: '{{{ITERATION_PLACEHOLDER}}}' \
                     always expands to the current iteration (like $JOULEX_ITERATION)"
                );
            }

            {
                let duplicates =
                    Self::find_duplicates(param_names_and_values.iter().map(|(name, _)| *name));
                if !duplicates.is_empty() {
                    bail!("Duplicate parameter names: {}", duplicates.join(", "));
                }
            }

            if matches.get_flag("aggregate-parameter-runs") {
                return Self::aggregated(
                    matches,
                    &command_strings,
                    &command_names,
                    param_names_and_values,
                    make_command,
                );
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

                // `index` always starts with the command dimension
                let Some((command_index, params_indices)) = index.split_first() else {
                    bail!("internal error: empty parameter index");
                };
                let parameters: Vec<_> = param_names_and_values
                    .iter()
                    .zip(params_indices)
                    .map(|((name, values), i)| (*name, values[*i].clone()))
                    .collect();
                commands.push(make_command(
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

            if matches.get_flag("expand-used-parameters") {
                let intermediate_templates: Vec<&str> = ["prepare", "conclude", "setup", "cleanup"]
                    .iter()
                    .filter_map(|arg| matches.get_many::<String>(arg))
                    .flatten()
                    .map(String::as_str)
                    .collect();
                commands = Self::keep_used_parameters(commands, &intermediate_templates);
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
                commands.push(make_command(command_names.get(i).copied(), s, Vec::new()));
            }
            Ok(Self(commands))
        }
    }

    /// `--aggregate-parameter-runs`: one command per template, whose runs go
    /// through all parameter combinations in turn.
    fn aggregated(
        matches: &ArgMatches,
        command_strings: &[&'a str],
        command_names: &[&'a str],
        params: Vec<(&'a str, Vec<ParameterValue>)>,
        make_command: impl Fn(Option<&'a str>, &'a str, Vec<ParameterNameAndValue<'a>>) -> Command<'a>,
    ) -> Result<Commands<'a>> {
        // Every value is run at least once, so this also bounds the run count
        let max = matches
            .get_one::<usize>("max-benchmarks")
            .copied()
            .unwrap_or(100_000);
        let combinations = params
            .iter()
            .try_fold(1usize, |acc, (_, values)| acc.checked_mul(values.len()))
            .filter(|&n| n <= max)
            .ok_or_else(|| {
                anyhow!(
                    "'--aggregate-parameter-runs' would need more than {max} runs per command \
                     (one per parameter combination). Reduce the ranges, or override with \
                     --max-benchmarks."
                )
            })?;
        if combinations == 0 {
            return Ok(Self(Vec::new()));
        }
        if command_names.len() > 1 && command_names.len() != command_strings.len() {
            return Err(OptionsError::UnexpectedCommandNameCount(
                command_names.len(),
                command_strings.len(),
            )
            .into());
        }

        let per_run = PerRunParameters::new(params, PerRunMode::Cycle);
        let commands = command_strings
            .iter()
            .enumerate()
            .map(|(i, template)| {
                let name = command_names.get(i).or(command_names.first()).copied();
                let mut command = make_command(name, template, Vec::new());
                command.per_run.push(per_run.clone());
                command
            })
            .collect();
        Ok(Self(commands))
    }

    /// `--expand-used-parameters`: drop the parameters a command doesn't use and
    /// remove the resulting duplicate benchmarks.
    ///
    /// A parameter is used by a command if `{name}` appears in its command
    /// template, its `--command-name`, or any `--prepare`, `--conclude`,
    /// `--setup` or `--cleanup` template (those can make otherwise identical
    /// command lines behave differently, so they keep all combinations).
    fn keep_used_parameters(
        commands: Vec<Command<'a>>,
        intermediate_templates: &[&str],
    ) -> Vec<Command<'a>> {
        let mut seen = std::collections::HashSet::new();
        commands
            .into_iter()
            .map(|mut command| {
                let expression = command.expression.clone();
                let name = command.name;
                command.parameters.retain(|(parameter, _)| {
                    let placeholder = format!("{{{parameter}}}");
                    expression.contains(&placeholder)
                        || name.is_some_and(|n| n.contains(&placeholder))
                        || intermediate_templates
                            .iter()
                            .any(|t| t.contains(&placeholder))
                });
                command
            })
            .filter(|command| {
                let key = (
                    command.expression.clone(),
                    command.get_name(),
                    command
                        .parameters
                        .iter()
                        .map(|(n, v)| (n.to_string(), v.to_string()))
                        .collect::<Vec<_>>(),
                );
                seen.insert(key)
            })
            .collect()
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

        let Some(step) = step else {
            return Err(ParameterScanError::StepRequired);
        };
        let step = Decimal::from_str(step)?;
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

#[test]
fn test_with_iteration_substitutes_placeholder() {
    let command = Command::new(None, "echo run-{iteration} {iteration}");
    assert_eq!(
        command.with_iteration(Some("3".into())).get_command_line(),
        "echo run-3 3"
    );
    // Without an iteration value (setup/cleanup), the command is unchanged
    assert_eq!(
        command.with_iteration(None).get_command_line(),
        "echo run-{iteration} {iteration}"
    );
    // Commands without the placeholder are borrowed, not cloned
    let plain = Command::new(None, "echo hi");
    assert!(matches!(
        plain.with_iteration(Some("0".into())),
        std::borrow::Cow::Borrowed(_)
    ));
    // The display name keeps the template
    assert_eq!(command.get_name(), "echo run-{iteration} {iteration}");
}

#[test]
fn test_iteration_is_a_reserved_parameter_name() {
    use crate::cli::get_cli_arguments;

    let matches = get_cli_arguments(vec!["joulex", "-L", "iteration", "1,2", "echo {iteration}"]);
    let err = Commands::from_cli_arguments(&matches).unwrap_err();
    assert!(err.to_string().contains("is reserved"));
}

#[test]
fn test_argv_command_is_executed_exactly() {
    let cmd = Command::from_argv(
        None,
        vec!["printf", "%s\n", "a b", "it's", "$HOME", "\\x"],
        Vec::new(),
    );
    let process = cmd.get_command().unwrap();
    assert_eq!(process.get_program(), "printf");
    assert_eq!(
        process.get_args().collect::<Vec<_>>(),
        ["%s\n", "a b", "it's", "$HOME", "\\x"]
    );
    assert_eq!(
        cmd.get_command_line(),
        "printf '%s\n' 'a b' 'it'\\''s' '$HOME' '\\x'"
    );
    assert_eq!(cmd.get_name(), cmd.get_command_line());
}

#[test]
fn test_argv_command_substitutes_parameters_per_argument() {
    let cmd = Command::from_argv(
        None,
        vec!["make", "-j{n}", "{target}"],
        vec![
            ("n", ParameterValue::Text("4".into())),
            ("target", ParameterValue::Text("a b; echo injected".into())),
            ("unused", ParameterValue::Text("x".into())),
        ],
    );
    let process = cmd.get_command().unwrap();
    assert_eq!(
        process.get_args().collect::<Vec<_>>(),
        ["-j4", "a b; echo injected"]
    );
    assert_eq!(cmd.get_command_line(), "make -j4 'a b; echo injected'");
    assert_eq!(
        cmd.get_unused_parameters()
            .map(|(p, _)| *p)
            .collect::<Vec<_>>(),
        ["unused"]
    );
}

#[test]
fn test_argv_from_cli_arguments() {
    let matches = crate::cli::get_cli_arguments(vec![
        "joulex", "-L", "n", "1,2", "--", "echo", "x {n}", "--flag",
    ]);
    let commands = Commands::from_cli_arguments(&matches).unwrap();
    let lines: Vec<_> = commands.iter().map(|c| c.get_command_line()).collect();
    assert_eq!(lines, ["echo 'x 1' --flag", "echo 'x 2' --flag"]);

    let matches = crate::cli::get_cli_arguments(vec!["joulex", "-n", "name", "--", "true"]);
    let commands = Commands::from_cli_arguments(&matches).unwrap();
    let names: Vec<_> = commands.iter().map(|c| c.get_name()).collect();
    assert_eq!(names, ["name"]);
}

#[test]
fn test_split_windows_command_line() {
    let split = |s: &str| split_windows_command_line(s);
    assert_eq!(
        split(r"C:\tools\app.exe --flag"),
        [r"C:\tools\app.exe", "--flag"]
    );
    assert_eq!(
        split(r#""C:\Program Files\app.exe" "a b"  c"#),
        [r"C:\Program Files\app.exe", "a b", "c"]
    );
    // Examples from the Microsoft documentation of the C runtime
    assert_eq!(split(r#"app "a b c" d e"#), ["app", "a b c", "d", "e"]);
    assert_eq!(
        split(r#"app "ab\"c" "\\" d"#),
        ["app", r#"ab"c"#, r"\", "d"]
    );
    assert_eq!(
        split(r#"app a\\\b d"e f"g h"#),
        ["app", r"a\\\b", "de fg", "h"]
    );
    assert_eq!(split(r#"app a\\\"b c d"#), ["app", r#"a\"b"#, "c", "d"]);
    assert_eq!(split(r#"app a\\\\"b c" d e"#), ["app", r"a\\b c", "d", "e"]);
    assert_eq!(split(r#"app a"b"" c d"#), ["app", r#"ab" c d"#]);
    // Empty quoted arguments are kept; surrounding whitespace is not
    assert_eq!(split("\t app \"\"  x "), ["app", "", "x"]);
    assert_eq!(split(""), Vec::<String>::new());
    assert_eq!(split("   "), Vec::<String>::new());
}

#[cfg(test)]
mod per_run_tests {
    use super::*;
    use crate::benchmark::executor::BenchmarkIteration;

    fn texts(values: &[&str]) -> Vec<ParameterValue> {
        values
            .iter()
            .map(|v| ParameterValue::Text(v.to_string()))
            .collect()
    }

    fn line(command: &Command, iteration: BenchmarkIteration) -> String {
        command.for_iteration(&iteration).get_command_line()
    }

    #[test]
    fn cycle_goes_through_all_combinations_in_order() {
        let per_run = PerRunParameters::new(
            vec![("a", texts(&["1", "2"])), ("b", texts(&["x", "y", "z"]))],
            PerRunMode::Cycle,
        );
        assert_eq!(per_run.combinations(), 6);
        let mut command = Command::new(None, "run {a}{b}");
        command.per_run.push(per_run);
        let lines: Vec<String> = (0..7)
            .map(|i| line(&command, BenchmarkIteration::Benchmark(i)))
            .collect();
        assert_eq!(
            lines,
            ["run 1x", "run 1y", "run 1z", "run 2x", "run 2y", "run 2z", "run 1x"]
        );
        assert_eq!(command.runs_multiple(), 6);
        // Setup/cleanup runs get no per-run values
        assert_eq!(
            line(&command, BenchmarkIteration::NonBenchmarkRun),
            "run {a}{b}"
        );
        assert_eq!(
            command.get_name_with_unused_parameters(),
            "run {a}{b} (aggregated over 6 parameter values)"
        );
    }

    #[test]
    fn random_draws_are_reproducible_and_paired() {
        let sample = |seed| {
            let mut command = Command::new(None, "cmd {f}");
            command.per_run.push(PerRunParameters::new(
                vec![("f", texts(&["a", "b", "c", "d"]))],
                PerRunMode::Random { seed },
            ));
            command
        };
        let sequence = |command: &Command, warmup: bool| -> Vec<String> {
            (0..40)
                .map(|i| {
                    let iteration = if warmup {
                        BenchmarkIteration::Warmup(i)
                    } else {
                        BenchmarkIteration::Benchmark(i)
                    };
                    line(command, iteration)
                })
                .collect()
        };
        let (first, second) = (sample(0), sample(0));
        // Same seed: same sequence, for any command (paired comparison)
        assert_eq!(sequence(&first, false), sequence(&second, false));
        // Another seed or the warmup stream: a different sequence
        assert_ne!(sequence(&first, false), sequence(&sample(1), false));
        assert_ne!(sequence(&first, false), sequence(&first, true));
        // All values occur
        for value in ["a", "b", "c", "d"] {
            let wanted = format!("cmd {value}");
            assert!(sequence(&first, false).contains(&wanted), "{value}");
        }
        // Sampling doesn't force whole cycles
        assert_eq!(first.runs_multiple(), 1);
        assert_eq!(
            first.get_name_with_unused_parameters(),
            "cmd {f} (sampled from 4 values)"
        );
    }

    #[test]
    fn intermediate_commands_share_the_per_run_values() {
        let mut command = Command::new(None, "run {f}");
        command.per_run.push(PerRunParameters::new(
            vec![("f", texts(&["x", "y", "z"]))],
            PerRunMode::Random { seed: 3 },
        ));
        let prepare = Command::new(None, "prepare {f}").with_per_run_parameters_of(&command);
        for i in 0..20 {
            let iteration = BenchmarkIteration::Benchmark(i);
            let run = line(&command, iteration);
            let prep = line(&prepare, BenchmarkIteration::Benchmark(i));
            assert_eq!(
                run.trim_start_matches("run "),
                prep.trim_start_matches("prepare ")
            );
        }
    }
}
