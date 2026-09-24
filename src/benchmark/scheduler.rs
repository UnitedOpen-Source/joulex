use super::benchmark_result::BenchmarkResult;
use super::executor::{Executor, MockExecutor, RawExecutor, ShellExecutor};
use super::{relative_speed, Benchmark, BenchmarkRunner};
use colored::*;
use std::cmp::Ordering;

use crate::command::{Command, Commands};
use crate::export::ExportManager;
use crate::options::{ExecutorKind, Options, OutputStyleOption, ScheduleMode, SortOrder};
use crate::output::progress_bar::get_progress_bar;

use anyhow::Result;

pub struct Scheduler<'a> {
    commands: &'a Commands<'a>,
    options: &'a Options,
    export_manager: &'a ExportManager,
    results: Vec<BenchmarkResult>,
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
        }
    }

    pub fn add_imported_results(&mut self, imported: Vec<BenchmarkResult>) {
        for res in imported {
            if self.options.output_style != OutputStyleOption::Disabled {
                println!(
                    "{}{}: {} (imported)",
                    "Benchmark ".bold(),
                    (self.results.len() + 1).to_string().bold(),
                    res.command_with_unused_parameters.cyan()
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
                runner.run_setup()?;
            }

            // 2. Warmup phase (interleaved)
            if self.options.warmup_count > 0 {
                let progress_bar = if self.options.output_style != OutputStyleOption::Disabled {
                    Some(get_progress_bar(
                        self.options.warmup_count * runners.len() as u64,
                        "Performing round-robin warmup runs",
                        self.options.output_style,
                    ))
                } else {
                    None
                };

                for w in 0..self.options.warmup_count {
                    for runner in &mut runners {
                        runner.run_warmup_iteration(w)?;
                        if let Some(bar) = progress_bar.as_ref() {
                            bar.inc(1);
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
                runner.run_initial_measurement()?;
                if let Some(bar) = progress_bar.as_ref() {
                    bar.inc(1);
                }
            }

            if let Some(bar) = progress_bar.as_ref() {
                bar.finish_and_clear();
            }

            let max_count = runners.iter().map(|r| r.count).max().unwrap_or(1);
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
            for i in 1..max_count {
                for runner in &mut runners {
                    if i < runner.count {
                        runner.run_timed_iteration(i)?;
                        if let Some(bar) = progress_bar.as_ref() {
                            bar.inc(1);
                        }
                    }
                }
            }

            if let Some(bar) = progress_bar.as_ref() {
                bar.finish_and_clear();
            }

            // 5. Cleanup phase
            for runner in &runners {
                runner.run_cleanup()?;
            }

            // 6. Finish and collect results
            for runner in runners {
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
                self.export_manager
                    .write_results(&intermediate_results, true)?;
            }
        } else {
            for (number, cmd) in commands_to_run {
                self.results.push(
                    Benchmark::new(
                        number,
                        number + display_offset,
                        cmd,
                        self.options,
                        &*executor,
                    )
                    .run()?,
                );

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
                self.export_manager
                    .write_results(&intermediate_results, true)?;
            }
        }

        Ok(())
    }

    pub fn print_relative_speed_comparison(&self) {
        if self.options.output_style == OutputStyleOption::Disabled {
            return;
        }

        let results: Vec<_> = if self.options.filter_failed {
            self.results.iter().filter(|r| !r.has_failure()).collect()
        } else {
            self.results.iter().collect()
        };

        if results.len() < 2 {
            return;
        }

        let results_slice: Vec<_> = results.iter().map(|r| (*r).clone()).collect();

        let reference = if self.options.reference_command.is_some() {
            // When a reference command is set, it's always the first result.
            // If it was filtered out, fall back to the fastest remaining result.
            let ref_cmd = &self.results[0];
            if self.options.filter_failed && ref_cmd.has_failure() {
                relative_speed::fastest_of(&results_slice)
            } else {
                &results_slice[0]
            }
        } else {
            relative_speed::fastest_of(&results_slice)
        };

        if let Some(annotated_results) = relative_speed::compute_with_check_from_reference(
            &results_slice,
            reference,
            self.options.sort_order_speed_comparison,
        ) {
            match self.options.sort_order_speed_comparison {
                SortOrder::MeanTime => {
                    println!("{}", "Summary".bold());

                    let reference = annotated_results.iter().find(|r| r.is_reference).unwrap();
                    let others = annotated_results.iter().filter(|r| !r.is_reference);

                    println!(
                        "  {} ran",
                        reference.result.command_with_unused_parameters.cyan()
                    );

                    for item in others {
                        let stddev = if let Some(stddev) = item.relative_speed_stddev {
                            format!(" ± {}", format!("{stddev:.2}").green())
                        } else {
                            "".into()
                        };
                        let comparator = match item.relative_ordering {
                            Ordering::Less => format!(
                                "{}{} times slower than",
                                format!("{:8.2}", item.relative_speed).bold().green(),
                                stddev
                            ),
                            Ordering::Greater => format!(
                                "{}{} times faster than",
                                format!("{:8.2}", item.relative_speed).bold().green(),
                                stddev
                            ),
                            Ordering::Equal => format!(
                                "    As fast ({}{}) as",
                                format!("{:.2}", item.relative_speed).bold().green(),
                                stddev
                            ),
                        };
                        println!(
                            "{} {}",
                            comparator,
                            item.result.command_with_unused_parameters.magenta()
                        );

                        if self.options.deep_stats {
                            if let (Some(ref_times), Some(item_times)) =
                                (&reference.result.times, &item.result.times)
                            {
                                match crate::stats::deep::compare_samples(ref_times, item_times) {
                                    Some(cmp) => {
                                        let sig_str = if cmp.is_significant_01 {
                                            "statistically significant (p < 0.01)".cyan()
                                        } else if cmp.is_significant_05 {
                                            "statistically significant (p < 0.05)".cyan()
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

                    if let Some(ref_cmd) = reference_command {
                        println!(
                            "{} (reference: {})",
                            "Relative speed comparison".bold(),
                            ref_cmd.cyan()
                        );
                    } else {
                        println!("{}", "Relative speed comparison".bold());
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
                            format!(" ± {}", format!("{stddev:5.2}").green())
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
                            format!("{:10.2}", item.relative_speed).bold().green(),
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
                 "Note".bold().red()
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
        self.export_manager.write_results(&results, false)
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
