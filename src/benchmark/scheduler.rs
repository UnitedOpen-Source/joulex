use super::benchmark_result::BenchmarkResult;
use super::executor::{Executor, MockExecutor, RawExecutor, ShellExecutor};
use super::{relative_speed, Benchmark, BenchmarkRunner};
use colored::Colorize as _;
use std::cmp::Ordering;

use crate::command::{Command, Commands};
use crate::export::ExportManager;
use crate::options::{ExecutorKind, Options, OutputStyleOption, ScheduleMode, SortOrder};
use crate::output::colors;
use crate::output::format::{format_duration, format_duration_unit};
use crate::output::progress_bar::get_progress_bar;

use anyhow::Result;

pub struct Scheduler<'a> {
    commands: &'a Commands<'a>,
    options: &'a Options,
    export_manager: &'a ExportManager,
    results: Vec<BenchmarkResult>,
    /// Index into `results` of the `--reference` command's result, if any.
    /// Imported results (`--import-json`) come first, so this is not
    /// necessarily 0.
    reference_index: Option<usize>,
    imported_count: usize,
}

impl<'a> Scheduler<'a> {
    pub fn new(
        commands: &'a Commands,
        options: &'a Options,
        export_manager: &'a ExportManager,
    ) -> Self {
        Self {
            commands,
            options,
            export_manager,
            results: vec![],
            reference_index: None,
            imported_count: 0,
        }
    }

    pub fn add_imported_results(&mut self, imported: Vec<BenchmarkResult>) {
        self.imported_count += imported.len();
        for res in imported {
            if self.options.output_style != OutputStyleOption::Disabled {
                println!(
                    "{}{}: {} (imported)",
                    "Benchmark ".bold(),
                    (self.results.len() + 1).to_string().bold(),
                    colors::cyan(&res.command_with_unused_parameters)
                );
            }
            self.results.push(res);
        }
    }

    pub fn run_benchmarks(&mut self) -> Result<()> {
        let reference = self
            .options
            .reference_command
            .as_ref()
            .map(|cmd| Command::new(self.options.reference_name.as_deref(), cmd));

        let total_live_commands = reference.iter().count() + self.commands.iter().count();
        if total_live_commands == 0 {
            return Ok(());
        }

        let mut executor: Box<dyn Executor> = match self.options.executor_kind {
            ExecutorKind::Raw => Box::new(RawExecutor::new(self.options)),
            ExecutorKind::Mock(ref shell) => Box::new(MockExecutor::new(shell.clone())),
            ExecutorKind::Shell(ref shell) => Box::new(ShellExecutor::new(shell, self.options)),
        };

        executor.calibrate()?;

        let display_offset = self.results.len();
        // The reference command (if any) is benchmarked first, right after the
        // imported results.
        self.reference_index = reference.as_ref().map(|_| display_offset);
        let commands_to_run: Vec<(usize, &Command)> = reference
            .iter()
            .chain(self.commands.iter())
            .enumerate()
            .collect();

        if self.options.schedule == ScheduleMode::RoundRobin && commands_to_run.len() > 1 {
            let mut runners: Vec<BenchmarkRunner> = commands_to_run
                .iter()
                .map(|&(number, cmd)| {
                    BenchmarkRunner::new(
                        number,
                        number + display_offset,
                        cmd,
                        self.options,
                        &*executor,
                    )
                })
                .collect();

            // 1. Setup phase for all commands
            for runner in &runners {
                if crate::util::interrupt::interrupted() {
                    break;
                }
                runner.run_setup()?;
            }

            // 2. Warmup phase: with `--warmup auto`, each command warms up until
            // it is stable (before interleaving starts); otherwise interleaved
            if self.options.warmup_auto {
                let progress_bar = if self.options.output_style != OutputStyleOption::Disabled {
                    Some(get_progress_bar(
                        super::AUTO_WARMUP_MAX_RUNS * runners.len() as u64,
                        "Performing warmup runs (auto)",
                        self.options.output_style,
                    ))
                } else {
                    None
                };
                for runner in &mut runners {
                    if crate::util::interrupt::interrupted() {
                        break;
                    }
                    match runner.run_auto_warmup(|| {
                        if let Some(bar) = progress_bar.as_ref() {
                            bar.inc(1);
                        }
                    }) {
                        Ok(w) => runner.warmup = Some(w),
                        Err(e) if e.is::<crate::error::Interrupted>() => break,
                        Err(e) => {
                            if let Some(bar) = progress_bar.as_ref() {
                                bar.finish_and_clear();
                            }
                            return Err(e);
                        }
                    }
                }
                if let Some(bar) = progress_bar.as_ref() {
                    bar.finish_and_clear();
                }
            } else if self.options.warmup_count > 0 {
                let progress_bar = if self.options.output_style != OutputStyleOption::Disabled {
                    Some(get_progress_bar(
                        self.options.warmup_count * runners.len() as u64,
                        "Performing round-robin warmup runs",
                        self.options.output_style,
                    ))
                } else {
                    None
                };

                'warmup: for w in 0..self.options.warmup_count {
                    for runner in &mut runners {
                        if crate::util::interrupt::interrupted() {
                            break 'warmup;
                        }
                        match runner.run_warmup_iteration(w) {
                            Ok(_) => {
                                if let Some(bar) = progress_bar.as_ref() {
                                    bar.inc(1);
                                }
                            }
                            Err(e) if e.is::<crate::error::Interrupted>() => break 'warmup,
                            Err(e) => {
                                if let Some(bar) = progress_bar.as_ref() {
                                    bar.finish_and_clear();
                                }
                                return Err(e);
                            }
                        }
                    }
                }

                if let Some(bar) = progress_bar.as_ref() {
                    bar.finish_and_clear();
                }
            }

            // 3. Initial measurement phase
            let progress_bar = if self.options.output_style != OutputStyleOption::Disabled {
                Some(get_progress_bar(
                    runners.len() as u64,
                    "Initial round-robin measurements",
                    self.options.output_style,
                ))
            } else {
                None
            };

            for runner in &mut runners {
                if crate::util::interrupt::interrupted() {
                    break;
                }
                match runner.run_initial_measurement() {
                    Ok(()) => {
                        if let Some(bar) = progress_bar.as_ref() {
                            bar.inc(1);
                        }
                    }
                    Err(e) if e.is::<crate::error::Interrupted>() => break,
                    Err(e) => {
                        if let Some(bar) = progress_bar.as_ref() {
                            bar.finish_and_clear();
                        }
                        return Err(e);
                    }
                }
            }

            if let Some(bar) = progress_bar.as_ref() {
                bar.finish_and_clear();
            }

            let common = match self.options.run_bounds {
                crate::options::RunBounds {
                    min,
                    max: Some(max),
                } if min == max => min,
                _ => {
                    let per_round: f64 = runners.iter().map(|r| r.initial_total_time).sum();
                    let n = if per_round > 0.0 {
                        (self.options.min_benchmarking_time / per_round) as u64
                    } else {
                        self.options.run_bounds.min
                    };
                    let n = n.max(self.options.run_bounds.min);
                    self.options.run_bounds.max.map_or(n, |m| n.min(m)).max(1)
                }
            };
            for r in &mut runners {
                r.count = common + r.extra_runs();
            }

            let max_count = runners.iter().map(|r| r.count).max().unwrap_or(common);
            let total_remaining_runs: u64 = runners.iter().map(|r| r.count.saturating_sub(1)).sum();

            let progress_bar = if self.options.output_style != OutputStyleOption::Disabled
                && total_remaining_runs > 0
            {
                Some(get_progress_bar(
                    total_remaining_runs,
                    "Performing round-robin benchmark runs",
                    self.options.output_style,
                ))
            } else {
                None
            };

            // 4. Interleaved timing iterations
            'timing: for i in 1..max_count {
                for runner in &mut runners {
                    if crate::util::interrupt::interrupted() {
                        break 'timing;
                    }
                    if i < runner.count {
                        match runner.run_timed_iteration(i) {
                            Ok(()) => {
                                if let Some(bar) = progress_bar.as_ref() {
                                    bar.inc(1);
                                }
                            }
                            Err(e) if e.is::<crate::error::Interrupted>() => break 'timing,
                            Err(e) => {
                                if let Some(bar) = progress_bar.as_ref() {
                                    bar.finish_and_clear();
                                }
                                return Err(e);
                            }
                        }
                    }
                }
            }

            if let Some(bar) = progress_bar.as_ref() {
                bar.finish_and_clear();
            }

            // 5. Cleanup phase
            for runner in &runners {
                if crate::util::interrupt::interrupted() {
                    let _ = runner.run_cleanup();
                } else {
                    runner.run_cleanup()?;
                }
            }

            // 6. Finish and collect results
            for runner in runners {
                if runner.times_real.is_empty() {
                    if self.options.output_style != OutputStyleOption::Disabled {
                        println!(
                            "{}{}: {} (interrupted before completing any runs)",
                            "Benchmark ".bold(),
                            (runner.display_number + 1).to_string().bold(),
                            colors::cyan(runner.command.get_name_with_unused_parameters())
                        );
                    }
                    continue;
                }
                let res = runner.finish(true)?;
                self.results.push(res);

                let intermediate_results: Vec<_> = if self.options.filter_failed {
                    self.results
                        .iter()
                        .filter(|r| !r.has_failure())
                        .cloned()
                        .collect()
                } else {
                    self.results.clone()
                };
                let reference = self.get_reference_result(&intermediate_results);
                self.export_manager
                    .write_results(&intermediate_results, reference, true)?;
            }
        } else {
            for (number, cmd) in commands_to_run {
                if crate::util::interrupt::interrupted() {
                    break;
                }
                if let Some(res) = Benchmark::new(
                    number,
                    number + display_offset,
                    cmd,
                    self.options,
                    &*executor,
                )
                .run()?
                {
                    self.results.push(res);

                    // We export results after each individual benchmark, because
                    // we would risk losing them if a later benchmark fails.
                    let intermediate_results: Vec<_> = if self.options.filter_failed {
                        self.results
                            .iter()
                            .filter(|r| !r.has_failure())
                            .cloned()
                            .collect()
                    } else {
                        self.results.clone()
                    };
                    let reference = self.get_reference_result(&intermediate_results);
                    self.export_manager
                        .write_results(&intermediate_results, reference, true)?;
                }
            }
        }

        Ok(())
    }

    fn get_reference_result<'b>(
        &self,
        results_slice: &'b [BenchmarkResult],
    ) -> Option<&'b BenchmarkResult> {
        self.reference_index
            .and_then(|idx| self.results.get(idx))
            .and_then(|ref_res| {
                results_slice
                    .iter()
                    .find(|r| r.command == ref_res.command && r.parameters == ref_res.parameters)
            })
    }

    pub fn print_relative_speed_comparison(&self) {
        if self.options.output_style == OutputStyleOption::Disabled {
            return;
        }

        // Keep track of each result's index in `self.results`, so that the
        // reference can be found again after filtering.
        let results: Vec<(usize, &BenchmarkResult)> = self
            .results
            .iter()
            .enumerate()
            .filter(|(_, r)| !(self.options.filter_failed && r.has_failure()))
            .collect();

        if results.len() < 2 {
            return;
        }

        let results_slice: Vec<_> = results.iter().map(|(_, r)| (*r).clone()).collect();

        // Use the `--reference` command's result if it is still present,
        // otherwise (no reference, or it was filtered out) the fastest one.
        let reference = self
            .get_reference_result(&results_slice)
            .unwrap_or_else(|| relative_speed::fastest_of(&results_slice));

        let interrupted = crate::util::interrupt::interrupted();
        let total_live_commands =
            self.options.reference_command.iter().count() + self.commands.iter().count();
        let completed_live_commands = self.results.len().saturating_sub(self.imported_count);
        let unbenchmarked = total_live_commands.saturating_sub(completed_live_commands);

        let summary_title = if interrupted {
            if unbenchmarked > 0 {
                let plural = if unbenchmarked == 1 {
                    "command"
                } else {
                    "commands"
                };
                format!("Summary (interrupted — {unbenchmarked} {plural} not benchmarked)")
            } else {
                "Summary (interrupted)".to_string()
            }
        } else {
            "Summary".to_string()
        };

        if let Some(annotated_results) = relative_speed::compute_with_check_from_reference(
            &results_slice,
            reference,
            self.options.sort_order_speed_comparison,
        ) {
            match self.options.sort_order_speed_comparison {
                SortOrder::MeanTime => {
                    println!("{}", summary_title.bold());

                    // `compute_with_check_from_reference` marks exactly one entry
                    let Some(reference) = annotated_results.iter().find(|r| r.is_reference) else {
                        return;
                    };
                    let others = annotated_results.iter().filter(|r| !r.is_reference);

                    println!(
                        "  {} ran",
                        colors::cyan(&reference.result.command_with_unused_parameters)
                    );

                    // All absolute numbers use one unit (the one of the largest mean,
                    // unless --time-unit is given), so the lines are directly comparable.
                    let largest_mean = annotated_results
                        .iter()
                        .map(|r| r.result.mean)
                        .fold(0.0, f64::max);
                    let (_, unit) = format_duration_unit(largest_mean, self.options.time_unit);

                    for item in others {
                        let absolute = absolute_difference(reference.result, item.result, unit);
                        let stddev = if let Some(stddev) = item.relative_speed_stddev {
                            format!(" ± {}", colors::green(format!("{stddev:.2}")))
                        } else {
                            "".into()
                        };
                        let comparator = match item.relative_ordering {
                            Ordering::Less => format!(
                                "{}{} times slower than",
                                colors::green(format!("{:8.2}", item.relative_speed)).bold(),
                                stddev
                            ),
                            Ordering::Greater => format!(
                                "{}{} times faster than",
                                colors::green(format!("{:8.2}", item.relative_speed)).bold(),
                                stddev
                            ),
                            Ordering::Equal => format!(
                                "    As fast ({}{}) as",
                                colors::green(format!("{:.2}", item.relative_speed)).bold(),
                                stddev
                            ),
                        };
                        println!(
                            "{} {} {}",
                            comparator,
                            colors::magenta(&item.result.command_with_unused_parameters),
                            absolute.dimmed()
                        );

                        if self.options.deep_stats {
                            if let (Some(ref_times), Some(item_times)) =
                                (&reference.result.times, &item.result.times)
                            {
                                match crate::stats::deep::compare_samples(ref_times, item_times) {
                                    Some(cmp) => {
                                        let sig_str = if cmp.is_significant_01 {
                                            colors::cyan("statistically significant (p < 0.01)")
                                        } else if cmp.is_significant_05 {
                                            colors::cyan("statistically significant (p < 0.05)")
                                        } else {
                                            "no statistically significant difference (p ≥ 0.05)"
                                                .dimmed()
                                        };
                                        println!(
                                            "      [Bootstrap t-test: t = {:.2}, p = {:.4} -> {}]",
                                            cmp.t_statistic, cmp.p_value, sig_str
                                        );
                                    }
                                    None => {
                                        println!(
                                            "      {}",
                                            "[Bootstrap t-test: not applicable (zero variance or insufficient samples)]"
                                                .dimmed()
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                SortOrder::Command => {
                    let show_reference_annotations = self.options.reference_command.is_some();
                    let reference_command = if show_reference_annotations {
                        annotated_results
                            .iter()
                            .find(|r| r.is_reference)
                            .map(|r| r.result.command_with_unused_parameters.as_str())
                    } else {
                        None
                    };

                    let interrupted_suffix = if interrupted {
                        if unbenchmarked > 0 {
                            let plural = if unbenchmarked == 1 {
                                "command"
                            } else {
                                "commands"
                            };
                            format!(" (interrupted — {unbenchmarked} {plural} not benchmarked)")
                        } else {
                            " (interrupted)".to_string()
                        }
                    } else {
                        String::new()
                    };

                    if let Some(ref_cmd) = reference_command {
                        println!(
                            "{} (reference: {}){}",
                            "Relative speed comparison".bold(),
                            colors::cyan(ref_cmd),
                            interrupted_suffix
                        );
                    } else {
                        println!(
                            "{}{}",
                            "Relative speed comparison".bold(),
                            interrupted_suffix
                        );
                    }

                    let max_cmd_len = annotated_results
                        .iter()
                        .map(|r| r.result.command_with_unused_parameters.len())
                        .max()
                        .unwrap_or(0);

                    for item in annotated_results {
                        let stddev_suffix = if item.is_reference {
                            "        ".into()
                        } else if let Some(stddev) = item.relative_speed_stddev {
                            format!(" ± {}", colors::green(format!("{stddev:5.2}")))
                        } else {
                            "        ".into()
                        };

                        let reference_annotation = if show_reference_annotations {
                            if item.is_reference {
                                format!("  {}", "(reference)".dimmed())
                            } else {
                                let ref_cmd = reference_command.unwrap();
                                let desc = match item.relative_ordering {
                                    Ordering::Less => {
                                        format!(
                                            "{:.2} times faster than {ref_cmd}",
                                            item.relative_speed
                                        )
                                    }
                                    Ordering::Greater => {
                                        format!(
                                            "{:.2} times slower than {ref_cmd}",
                                            item.relative_speed
                                        )
                                    }
                                    Ordering::Equal => format!("as fast as {ref_cmd}"),
                                };
                                format!("  {desc}")
                            }
                        } else {
                            String::new()
                        };

                        println!(
                            "  {}{}  {:<width$}{}",
                            colors::green(format!("{:10.2}", item.relative_speed)).bold(),
                            stddev_suffix,
                            item.result.command_with_unused_parameters,
                            reference_annotation,
                            width = max_cmd_len,
                        );
                    }
                }
            }
        } else {
            eprintln!(
                "{}: The benchmark comparison could not be computed as some benchmark times are zero. \
                 This could be caused by background interference during the initial calibration phase \
                 of joulex, in combination with very fast commands (faster than a few milliseconds). \
                 Try to re-run the benchmark on a quiet system. If you did not do so already, try the \
                 --shell=none/-N option. If it does not help either, you command is most likely too fast \
                 to be accurately benchmarked by joulex.",
                 colors::red("Note").bold()
            );
        }
    }

    pub fn final_export(&self) -> Result<()> {
        let results: Vec<_> = if self.options.filter_failed {
            self.results
                .iter()
                .filter(|r| !r.has_failure())
                .cloned()
                .collect()
        } else {
            self.results.clone()
        };
        let reference = self.get_reference_result(&results);
        self.export_manager
            .write_results(&results, reference, false)
    }
}

#[cfg(test)]
fn generate_results(args: &[&'static str]) -> Result<Vec<BenchmarkResult>> {
    use crate::cli::get_cli_arguments;

    let args = ["hyperfine", "--debug-mode", "--style=none"]
        .iter()
        .chain(args);
    let cli_arguments = get_cli_arguments(args);
    let mut options = Options::from_cli_arguments(&cli_arguments)?;

    assert_eq!(options.executor_kind, ExecutorKind::Mock(None));

    let commands = Commands::from_cli_arguments(&cli_arguments)?;
    let export_manager = ExportManager::from_cli_arguments(
        &cli_arguments,
        options.time_unit,
        options.sort_order_exports,
    )?;

    options.validate_against_command_list(&commands)?;

    let mut scheduler = Scheduler::new(&commands, &options, &export_manager);

    scheduler.run_benchmarks()?;
    Ok(scheduler.results)
}

#[test]
fn scheduler_basic() -> Result<()> {
    insta::assert_yaml_snapshot!(generate_results(&["--runs=2", "sleep 0.123", "sleep 0.456"])?, @r#"
    - command: sleep 0.123
      mean: 0.123
      stddev: 0
      median: 0.123
      percentiles:
        p05: 0.123
        p25: 0.123
        p75: 0.123
        p95: 0.123
      geometric_mean: 0.123
      user: 0
      system: 0
      cpu_percent: 0
      min: 0.123
      max: 0.123
      times:
        - 0.123
        - 0.123
      user_times:
        - 0
        - 0
      system_times:
        - 0
        - 0
      memory_usage_byte:
        - 0
        - 0
      exit_codes:
        - 0
        - 0
    - command: sleep 0.456
      mean: 0.456
      stddev: 0
      median: 0.456
      percentiles:
        p05: 0.456
        p25: 0.456
        p75: 0.456
        p95: 0.456
      geometric_mean: 0.456
      user: 0
      system: 0
      cpu_percent: 0
      min: 0.456
      max: 0.456
      times:
        - 0.456
        - 0.456
      user_times:
        - 0
        - 0
      system_times:
        - 0
        - 0
      memory_usage_byte:
        - 0
        - 0
      exit_codes:
        - 0
        - 0
    "#);

    Ok(())
}

#[test]
fn scheduler_round_robin() -> Result<()> {
    insta::assert_yaml_snapshot!(generate_results(&["--schedule=round-robin", "--runs=2", "sleep 0.123", "sleep 0.456"])?, @r#"
    - command: sleep 0.123
      mean: 0.123
      stddev: 0
      median: 0.123
      percentiles:
        p05: 0.123
        p25: 0.123
        p75: 0.123
        p95: 0.123
      geometric_mean: 0.123
      user: 0
      system: 0
      cpu_percent: 0
      min: 0.123
      max: 0.123
      times:
        - 0.123
        - 0.123
      user_times:
        - 0
        - 0
      system_times:
        - 0
        - 0
      memory_usage_byte:
        - 0
        - 0
      exit_codes:
        - 0
        - 0
    - command: sleep 0.456
      mean: 0.456
      stddev: 0
      median: 0.456
      percentiles:
        p05: 0.456
        p25: 0.456
        p75: 0.456
        p95: 0.456
      geometric_mean: 0.456
      user: 0
      system: 0
      cpu_percent: 0
      min: 0.456
      max: 0.456
      times:
        - 0.456
        - 0.456
      user_times:
        - 0
        - 0
      system_times:
        - 0
        - 0
      memory_usage_byte:
        - 0
        - 0
      exit_codes:
        - 0
        - 0
    "#);

    Ok(())
}

/// "(12.3 ms, +4.5 ms)": the mean of `item` and its difference to the
/// reference, plus energy when both results have it. Positive differences mean
/// `item` is slower (or uses more energy) than the reference.
fn absolute_difference(
    reference: &BenchmarkResult,
    item: &BenchmarkResult,
    unit: crate::util::units::Unit,
) -> String {
    let signed = |value: f64, formatted: String| {
        if value >= 0.0 {
            format!("+{formatted}")
        } else {
            format!("−{formatted}")
        }
    };
    let time_diff = item.mean - reference.mean;
    let mut parts = vec![
        format_duration(item.mean, Some(unit)),
        signed(time_diff, format_duration(time_diff.abs(), Some(unit))),
    ];
    if let (Some(e_ref), Some(e_item)) = (reference.mean_energy_joules, item.mean_energy_joules) {
        let energy_diff = e_item - e_ref;
        parts.push(format!(
            "{e_item:.3} J, {}",
            signed(energy_diff, format!("{:.3} J", energy_diff.abs()))
        ));
    }
    format!("({})", parts.join(", "))
}

#[cfg(test)]
mod absolute_difference_tests {
    use super::*;
    use crate::util::units::Unit;

    fn result(mean: f64, energy: Option<f64>) -> BenchmarkResult {
        BenchmarkResult {
            mean,
            mean_energy_joules: energy,
            ..Default::default()
        }
    }

    #[test]
    fn slower_item_has_positive_difference() {
        assert_eq!(
            absolute_difference(
                &result(0.0557, None),
                &result(0.1046, None),
                Unit::MilliSecond
            ),
            "(104.6 ms, +48.9 ms)"
        );
    }

    #[test]
    fn faster_item_has_negative_difference() {
        assert_eq!(
            absolute_difference(&result(2.0, None), &result(1.5, None), Unit::Second),
            "(1.500 s, −0.500 s)"
        );
    }

    #[test]
    fn energy_is_included_when_both_have_it() {
        assert_eq!(
            absolute_difference(
                &result(1.0, Some(0.66)),
                &result(2.0, Some(1.21)),
                Unit::Second
            ),
            "(2.000 s, +1.000 s, 1.210 J, +0.550 J)"
        );
        assert_eq!(
            absolute_difference(&result(1.0, Some(0.66)), &result(2.0, None), Unit::Second),
            "(2.000 s, +1.000 s)"
        );
    }
}
