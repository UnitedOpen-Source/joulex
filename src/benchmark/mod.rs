pub mod benchmark_result;
pub mod executor;
pub mod relative_speed;
pub mod scheduler;
pub mod timing_result;

use std::cmp;

use crate::benchmark::executor::BenchmarkIteration;
use crate::command::Command;
use crate::energy::{get_energy_sampler, EnergySampler};
use crate::options::{
    CmdFailureAction, CommandOutputPolicy, ExecutorKind, Options, OutputStyleOption,
};
use crate::outlier_detection::{modified_zscores, OUTLIER_THRESHOLD};
use crate::output::format::{format_duration, format_duration_unit};
use crate::output::progress_bar::get_progress_bar;
use crate::output::warnings::{OutlierWarningOptions, Warnings};
use crate::stats::deep::compute_deep_stats;
use crate::util::exit_code::extract_exit_code;
use crate::util::min_max::{max, min};
use crate::util::units::{format_bytes, Second};
use benchmark_result::BenchmarkResult;
use timing_result::TimingResult;

use anyhow::{anyhow, Result};
use colored::*;
use statistical::{mean, median, standard_deviation};

use self::executor::Executor;

/// Threshold for warning about fast execution time
pub const MIN_EXECUTION_TIME: Second = 5e-3;

/// Manages the state, execution steps, and metric collection for a single benchmark command.
pub struct BenchmarkRunner<'a> {
    pub number: usize,
    pub display_number: usize,
    pub command: &'a Command<'a>,
    pub options: &'a Options,
    pub executor: &'a dyn Executor,
    pub output_policy: &'a CommandOutputPolicy,
    pub preparation_command: Option<Command<'a>>,
    pub conclusion_command: Option<Command<'a>>,
    pub energy_sampler: Option<Box<dyn EnergySampler>>,
    pub times_real: Vec<Second>,
    pub times_user: Vec<Second>,
    pub times_system: Vec<Second>,
    pub memory_usage_byte: Vec<u64>,
    pub energy_measurements: Vec<f64>,
    pub exit_codes: Vec<Option<i32>>,
    pub all_succeeded: bool,
    pub count: u64,
}

impl<'a> BenchmarkRunner<'a> {
    pub fn new(
        number: usize,
        display_number: usize,
        command: &'a Command<'a>,
        options: &'a Options,
        executor: &'a dyn Executor,
    ) -> Self {
        let output_policy = &options.command_output_policies[number];

        let preparation_command = options.preparation_command.as_ref().map(|values| {
            let preparation_command = if values.len() == 1 {
                &values[0]
            } else {
                &values[number]
            };
            Command::new_parametrized(
                None,
                preparation_command,
                command.get_parameters().iter().cloned(),
            )
        });

        let conclusion_command = options.conclusion_command.as_ref().map(|values| {
            let conclusion_command = if values.len() == 1 {
                &values[0]
            } else {
                &values[number]
            };
            Command::new_parametrized(
                None,
                conclusion_command,
                command.get_parameters().iter().cloned(),
            )
        });

        let energy_sampler = if options.measure_energy {
            Some(get_energy_sampler())
        } else {
            None
        };

        BenchmarkRunner {
            number,
            display_number,
            command,
            options,
            executor,
            output_policy,
            preparation_command,
            conclusion_command,
            energy_sampler,
            times_real: vec![],
            times_user: vec![],
            times_system: vec![],
            memory_usage_byte: vec![],
            energy_measurements: vec![],
            exit_codes: vec![],
            all_succeeded: true,
            count: 0,
        }
    }

    fn run_intermediate_command(
        &self,
        command: &Command<'_>,
        iteration: executor::BenchmarkIteration,
        error_output: &'static str,
    ) -> Result<TimingResult> {
        self.executor
            .run_command_and_measure(
                command,
                iteration,
                Some(CmdFailureAction::RaiseError),
                self.output_policy,
            )
            .map(|r| r.0)
            .map_err(|_| anyhow!(error_output))
    }

    pub fn run_setup(&self) -> Result<TimingResult> {
        let command = self.options.setup_command.as_ref().map(|setup_command| {
            Command::new_parametrized(
                None,
                setup_command,
                self.command.get_parameters().iter().cloned(),
            )
        });

        let error_output = "The setup command terminated with a non-zero exit code. \
                            Append ' || true' to the command if you are sure that this can be ignored.";

        Ok(command
            .map(|cmd| {
                self.run_intermediate_command(
                    &cmd,
                    executor::BenchmarkIteration::NonBenchmarkRun,
                    error_output,
                )
            })
            .transpose()?
            .unwrap_or_default())
    }

    pub fn run_cleanup(&self) -> Result<TimingResult> {
        let command = self
            .options
            .cleanup_command
            .as_ref()
            .map(|cleanup_command| {
                Command::new_parametrized(
                    None,
                    cleanup_command,
                    self.command.get_parameters().iter().cloned(),
                )
            });

        let error_output = "The cleanup command terminated with a non-zero exit code. \
                            Append ' || true' to the command if you are sure that this can be ignored.";

        Ok(command
            .map(|cmd| {
                self.run_intermediate_command(
                    &cmd,
                    executor::BenchmarkIteration::NonBenchmarkRun,
                    error_output,
                )
            })
            .transpose()?
            .unwrap_or_default())
    }

    pub fn run_preparation(&self, iteration: BenchmarkIteration) -> Result<Option<TimingResult>> {
        let error_output = "The preparation command terminated with a non-zero exit code. \
                            Append ' || true' to the command if you are sure that this can be ignored.";

        self.preparation_command
            .as_ref()
            .map(|cmd| self.run_intermediate_command(cmd, iteration, error_output))
            .transpose()
    }

    pub fn run_conclusion(&self, iteration: BenchmarkIteration) -> Result<Option<TimingResult>> {
        let error_output = "The conclusion command terminated with a non-zero exit code. \
                            Append ' || true' to the command if you are sure that this can be ignored.";

        self.conclusion_command
            .as_ref()
            .map(|cmd| self.run_intermediate_command(cmd, iteration, error_output))
            .transpose()
    }

    pub fn run_warmup_iteration(&mut self, iteration: u64) -> Result<()> {
        let _ = self.run_preparation(BenchmarkIteration::Warmup(iteration))?;
        let _ = self.executor.run_command_and_measure(
            self.command,
            BenchmarkIteration::Warmup(iteration),
            None,
            self.output_policy,
        )?;
        let _ = self.run_conclusion(BenchmarkIteration::Warmup(iteration))?;
        Ok(())
    }

    pub fn run_initial_measurement(&mut self) -> Result<()> {
        let preparation_result = self.run_preparation(BenchmarkIteration::Benchmark(0))?;
        let preparation_overhead =
            preparation_result.map_or(0.0, |res| res.time_real + self.executor.time_overhead());

        if let Some(sampler) = self.energy_sampler.as_mut() {
            sampler.start();
        }
        let (res, status) = self.executor.run_command_and_measure(
            self.command,
            BenchmarkIteration::Benchmark(0),
            None,
            self.output_policy,
        )?;
        let energy_initial = self.energy_sampler.as_mut().and_then(|s| s.stop());
        let success = status.success();

        let conclusion_result = self.run_conclusion(BenchmarkIteration::Benchmark(0))?;
        let conclusion_overhead =
            conclusion_result.map_or(0.0, |res| res.time_real + self.executor.time_overhead());

        let runs_in_min_time = (self.options.min_benchmarking_time
            / (res.time_real
                + self.executor.time_overhead()
                + preparation_overhead
                + conclusion_overhead)) as u64;

        let count = {
            let min = cmp::max(runs_in_min_time, self.options.run_bounds.min);

            let count = self
                .options
                .run_bounds
                .max
                .as_ref()
                .map(|max| cmp::min(min, *max))
                .unwrap_or(min);

            cmp::max(count, 1)
        };

        self.count = count;

        self.times_real.push(res.time_real);
        self.times_user.push(res.time_user);
        self.times_system.push(res.time_system);
        self.memory_usage_byte.push(res.memory_usage_byte);
        if let Some(e) = energy_initial {
            self.energy_measurements.push(e);
        }
        self.exit_codes.push(extract_exit_code(status));
        self.all_succeeded = self.all_succeeded && success;

        Ok(())
    }

    pub fn run_timed_iteration(&mut self, iteration: u64) -> Result<()> {
        self.run_preparation(BenchmarkIteration::Benchmark(iteration))?;

        if let Some(sampler) = self.energy_sampler.as_mut() {
            sampler.start();
        }
        let (res, status) = self.executor.run_command_and_measure(
            self.command,
            BenchmarkIteration::Benchmark(iteration),
            None,
            self.output_policy,
        )?;
        let energy = self.energy_sampler.as_mut().and_then(|s| s.stop());
        let success = status.success();

        self.times_real.push(res.time_real);
        self.times_user.push(res.time_user);
        self.times_system.push(res.time_system);
        self.memory_usage_byte.push(res.memory_usage_byte);
        if let Some(e) = energy {
            self.energy_measurements.push(e);
        }
        self.exit_codes.push(extract_exit_code(status));
        self.all_succeeded = self.all_succeeded && success;

        self.run_conclusion(BenchmarkIteration::Benchmark(iteration))?;

        Ok(())
    }

    pub fn finish(self, print_header: bool) -> Result<BenchmarkResult> {
        let t_num = self.times_real.len();
        let t_mean = mean(&self.times_real);
        let t_stddev = if self.times_real.len() > 1 {
            Some(standard_deviation(&self.times_real, Some(t_mean)))
        } else {
            None
        };
        let t_median = median(&self.times_real);
        let t_min = min(&self.times_real);
        let t_max = max(&self.times_real);

        let user_mean = mean(&self.times_user);
        let system_mean = mean(&self.times_system);

        let (mean_str, time_unit) = format_duration_unit(t_mean, self.options.time_unit);
        let min_str = format_duration(t_min, Some(time_unit));
        let max_str = format_duration(t_max, Some(time_unit));
        let num_str = format!("{t_num} runs");

        let user_str = format_duration(user_mean, Some(time_unit));
        let system_str = format_duration(system_mean, Some(time_unit));

        let peak_memory = self.memory_usage_byte.iter().copied().max().unwrap_or(0);
        let mem_str = if peak_memory > 0 {
            format!(", Peak Memory: {}", format_bytes(peak_memory))
        } else {
            String::new()
        };

        let total_cpu = user_mean + system_mean;
        let cpu_percent = Some(if t_mean > 0.0 {
            (total_cpu / t_mean) * 100.0
        } else {
            0.0
        });
        let cpu_str = if let Some(pct) = cpu_percent {
            format!(", CPU: {:.0}%", pct)
        } else {
            String::new()
        };

        if self.options.output_style != OutputStyleOption::Disabled {
            if print_header {
                println!(
                    "{}{}: {}",
                    "Benchmark ".bold(),
                    (self.display_number + 1).to_string().bold(),
                    self.command.get_name_with_unused_parameters(),
                );
            }

            if self.times_real.len() == 1 {
                println!(
                    "  Time ({} ≡):        {:>8}  {:>8}     [User: {}, System: {}{}{}]",
                    "abs".green().bold(),
                    mean_str.green().bold(),
                    "        ", // alignment
                    user_str.blue(),
                    system_str.blue(),
                    cpu_str.blue(),
                    mem_str.blue()
                );
            } else {
                let stddev_str = format_duration(t_stddev.unwrap(), Some(time_unit));

                println!(
                    "  Time ({} ± {}):     {:>8} ± {:>8}    [User: {}, System: {}{}{}]",
                    "mean".green().bold(),
                    "σ".green(),
                    mean_str.green().bold(),
                    stddev_str.green(),
                    user_str.blue(),
                    system_str.blue(),
                    cpu_str.blue(),
                    mem_str.blue()
                );

                println!(
                    "  Range ({} … {}):   {:>8} … {:>8}    {}",
                    "min".cyan(),
                    "max".purple(),
                    min_str.cyan(),
                    max_str.purple(),
                    num_str.dimmed()
                );
            }

            if self.options.measure_energy {
                if !self.energy_measurements.is_empty() {
                    let mean_joules = mean(&self.energy_measurements);
                    let stddev_joules = if self.energy_measurements.len() > 1 {
                        Some(standard_deviation(
                            &self.energy_measurements,
                            Some(mean_joules),
                        ))
                    } else {
                        None
                    };
                    let watts = if t_mean > 0.0 {
                        mean_joules / t_mean
                    } else {
                        0.0
                    };

                    let energy_str = if let Some(sd) = stddev_joules {
                        format!("{:.3} ± {:.3} J", mean_joules, sd)
                    } else {
                        format!("{:.3} J", mean_joules)
                    };

                    println!(
                        "  Energy ({}):        {:>14}    [Power: {}]",
                        "mean".yellow().bold(),
                        energy_str.yellow().bold(),
                        format!("{:.2} W", watts).yellow()
                    );
                } else {
                    println!(
                        "  Energy:             {:>14}",
                        "RAPL unprivileged/unavailable on host".dimmed()
                    );
                }
            }

            if self.options.deep_stats {
                if let Some(deep) = compute_deep_stats(&self.times_real) {
                    println!(
                        "  Bootstrap 95% CI:   [mean: {:.4}s … {:.4}s, median: {:.4}s … {:.4}s]",
                        deep.mean_ci_lower,
                        deep.mean_ci_upper,
                        deep.median_ci_lower,
                        deep.median_ci_upper
                    );
                }
            }
        }

        // Warnings
        let mut warnings = vec![];

        // Check execution time
        if matches!(self.options.executor_kind, ExecutorKind::Shell(_))
            && self.times_real.iter().any(|&t| t < MIN_EXECUTION_TIME)
        {
            warnings.push(Warnings::FastExecutionTime);
        }

        // Check program exit codes
        if !self.all_succeeded {
            warnings.push(Warnings::NonZeroExitCode);
        }

        // Run outlier detection
        let scores = modified_zscores(&self.times_real);

        let outlier_warning_options = OutlierWarningOptions {
            warmup_in_use: self.options.warmup_count > 0,
            prepare_in_use: self
                .options
                .preparation_command
                .as_ref()
                .map(|v| v.len())
                .unwrap_or(0)
                > 0,
        };

        if !self.options.suppress_outlier_warnings {
            let total_cpu = user_mean + system_mean;
            if t_mean >= 0.1 && (total_cpu <= 0.0 || t_mean >= 5.0 * total_cpu) {
                let ratio = if total_cpu > 0.0 {
                    t_mean / total_cpu
                } else {
                    f64::INFINITY
                };
                warnings.push(Warnings::OffCpuTime(t_mean, total_cpu, ratio));
            }

            if scores[0] > OUTLIER_THRESHOLD {
                warnings.push(Warnings::SlowInitialRun(
                    self.times_real[0],
                    outlier_warning_options,
                ));
            } else if scores.iter().any(|&s| s.abs() > OUTLIER_THRESHOLD) {
                warnings.push(Warnings::OutliersDetected(outlier_warning_options));
            }
        }

        if !warnings.is_empty() {
            eprintln!(" ");

            for warning in &warnings {
                eprintln!("  {}: {}", "Warning".yellow(), warning);
            }
        }

        if self.options.output_style != OutputStyleOption::Disabled {
            println!(" ");
        }

        let (mean_energy, mean_watts, energy_all) = if !self.energy_measurements.is_empty() {
            let m_j = mean(&self.energy_measurements);
            let m_w = if t_mean > 0.0 { m_j / t_mean } else { 0.0 };
            (Some(m_j), Some(m_w), Some(self.energy_measurements))
        } else {
            (None, None, None)
        };

        Ok(BenchmarkResult {
            command: self.command.get_name(),
            command_with_unused_parameters: self.command.get_name_with_unused_parameters(),
            mean: t_mean,
            stddev: t_stddev,
            median: t_median,
            user: user_mean,
            system: system_mean,
            cpu_percent,
            min: t_min,
            max: t_max,
            times: Some(self.times_real),
            user_times: Some(self.times_user),
            system_times: Some(self.times_system),
            memory_usage_byte: Some(self.memory_usage_byte),
            mean_energy_joules: mean_energy,
            mean_watts,
            energy_joules: energy_all,
            exit_codes: self.exit_codes,
            parameters: self
                .command
                .get_parameters()
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
        })
    }
}

pub struct Benchmark<'a> {
    number: usize,
    display_number: usize,
    command: &'a Command<'a>,
    options: &'a Options,
    executor: &'a dyn Executor,
}

impl<'a> Benchmark<'a> {
    pub fn new(
        number: usize,
        display_number: usize,
        command: &'a Command<'a>,
        options: &'a Options,
        executor: &'a dyn Executor,
    ) -> Self {
        Benchmark {
            number,
            display_number,
            command,
            options,
            executor,
        }
    }

    /// Run the benchmark for a single command in grouped mode
    pub fn run(&self) -> Result<BenchmarkResult> {
        let mut runner = BenchmarkRunner::new(
            self.number,
            self.display_number,
            self.command,
            self.options,
            self.executor,
        );

        if self.options.output_style != OutputStyleOption::Disabled {
            println!(
                "{}{}: {}",
                "Benchmark ".bold(),
                (self.display_number + 1).to_string().bold(),
                self.command.get_name_with_unused_parameters(),
            );
        }

        runner.run_setup()?;

        // Warmup phase
        if self.options.warmup_count > 0 {
            let progress_bar = if self.options.output_style != OutputStyleOption::Disabled {
                Some(get_progress_bar(
                    self.options.warmup_count,
                    "Performing warmup runs",
                    self.options.output_style,
                ))
            } else {
                None
            };

            for i in 0..self.options.warmup_count {
                runner.run_warmup_iteration(i)?;
                if let Some(bar) = progress_bar.as_ref() {
                    bar.inc(1)
                }
            }
            if let Some(bar) = progress_bar.as_ref() {
                bar.finish_and_clear()
            }
        }

        // Set up progress bar (and spinner for initial measurement)
        let progress_bar = if self.options.output_style != OutputStyleOption::Disabled {
            Some(get_progress_bar(
                self.options.run_bounds.min,
                "Initial time measurement",
                self.options.output_style,
            ))
        } else {
            None
        };

        runner.run_initial_measurement()?;

        let count = runner.count;
        let count_remaining = count - 1;

        // Re-configure the progress bar
        if let Some(bar) = progress_bar.as_ref() {
            bar.set_length(count)
        }
        if let Some(bar) = progress_bar.as_ref() {
            bar.inc(1)
        }

        // Gather statistics (perform the actual benchmark)
        for i in 0..count_remaining {
            let msg = {
                let mean = format_duration(mean(&runner.times_real), self.options.time_unit);
                format!("Current estimate: {}", mean.to_string().green())
            };

            if let Some(bar) = progress_bar.as_ref() {
                bar.set_message(msg.to_owned())
            }

            runner.run_timed_iteration(i + 1)?;

            if let Some(bar) = progress_bar.as_ref() {
                bar.inc(1)
            }
        }

        if let Some(bar) = progress_bar.as_ref() {
            bar.finish_and_clear()
        }

        runner.run_cleanup()?;

        runner.finish(false)
    }
}
