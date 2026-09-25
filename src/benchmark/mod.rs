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
    CmdFailureAction, CommandOutputPolicy, ExecutorKind, FirstRunPolicy, Options, OutputStyleOption,
};
use crate::outlier_detection::{
    modified_zscores, outlier_indices, MAX_DISCARD_FRACTION, OUTLIER_THRESHOLD,
};
use crate::output::format::{format_duration, format_duration_unit};
use crate::output::progress_bar::get_progress_bar;
use crate::output::warnings::{OutlierWarningOptions, Warnings};
use crate::stats::deep::compute_deep_stats;
use crate::stats::diagnostics::Diagnostics;
use crate::util::exit_code::extract_exit_code;
use crate::util::min_max::{max, min};
use crate::util::units::{format_bytes, Second};
use benchmark_result::{BenchmarkResult, FirstRun, OmittedRun};
use timing_result::TimingResult;

use crate::output::colors;
use crate::stats::basic::{mean, median, standard_deviation};
use anyhow::{anyhow, bail, Result};
use colored::Colorize as _;

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
    pub energy_measurements: Vec<Option<f64>>,
    pub exit_codes: Vec<Option<i32>>,
    /// OS resource counters per run (`None` where unavailable, e.g. Windows)
    pub resource_counters: Vec<Option<timing_result::ResourceCounters>>,
    pub all_succeeded: bool,
    pub count: u64,
    /// Number of warmup runs performed and, for `--warmup auto`, whether the
    /// timings stabilized
    pub warmup: Option<WarmupSummary>,
    pub initial_total_time: f64,
    /// `--first-run=separate` with warmup: wall-clock time of the first warmup run
    pub cold_warmup_time: Option<Second>,
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
            .with_per_run_parameters_of(command)
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
            .with_per_run_parameters_of(command)
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
            resource_counters: vec![],
            all_succeeded: true,
            count: 0,
            warmup: None,
            initial_total_time: 0.0,
            cold_warmup_time: None,
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
        let command = self.options.setup_command.as_ref().map(|values| {
            Command::new_parametrized(
                None,
                per_command(values, self.number),
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
        let command = self.options.cleanup_command.as_ref().map(|values| {
            Command::new_parametrized(
                None,
                per_command(values, self.number),
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

    /// Perform one warmup run and return its wall-clock time.
    pub fn run_warmup_iteration(&mut self, iteration: u64) -> Result<Second> {
        let _ = self.run_preparation(BenchmarkIteration::Warmup(iteration))?;
        let (result, _) = self.executor.run_command_and_measure(
            self.command,
            BenchmarkIteration::Warmup(iteration),
            None,
            self.output_policy,
        )?;
        let _ = self.run_conclusion(BenchmarkIteration::Warmup(iteration))?;
        if iteration == 0 && self.options.first_run == FirstRunPolicy::Separate {
            self.cold_warmup_time = Some(result.time_real);
        }
        Ok(result.time_real)
    }

    /// `--warmup auto`: perform warmup runs until the last
    /// `AUTO_WARMUP_WINDOW` timings are stable (relative spread at most
    /// `AUTO_WARMUP_THRESHOLD`), at most `AUTO_WARMUP_MAX_RUNS` times.
    pub fn run_auto_warmup(&mut self, mut on_run: impl FnMut()) -> Result<WarmupSummary> {
        let mut times = Vec::new();
        for iteration in 0..AUTO_WARMUP_MAX_RUNS {
            if crate::util::interrupt::interrupted() {
                anyhow::bail!(crate::error::Interrupted);
            }
            times.push(self.run_warmup_iteration(iteration)?);
            on_run();
            if times.len() >= AUTO_WARMUP_WINDOW {
                let spread = relative_spread(&times[times.len() - AUTO_WARMUP_WINDOW..]);
                if spread <= AUTO_WARMUP_THRESHOLD {
                    return Ok(WarmupSummary {
                        runs: times.len() as u64,
                        auto: true,
                        stable: true,
                        spread,
                    });
                }
            }
        }
        let window = &times[times.len().saturating_sub(AUTO_WARMUP_WINDOW)..];
        Ok(WarmupSummary {
            runs: times.len() as u64,
            auto: true,
            stable: false,
            spread: relative_spread(window),
        })
    }

    fn has_warmup(&self) -> bool {
        self.options.warmup_auto || self.options.warmup_count > 0
    }

    /// Whether the first timing run is excluded from the statistics
    /// (`--first-run=separate|discard`). With warmup, `separate` reports the
    /// first warmup run instead, so no timing run is excluded.
    fn excludes_first_timing_run(&self) -> bool {
        match self.options.first_run {
            FirstRunPolicy::Include => false,
            FirstRunPolicy::Separate => !self.has_warmup(),
            FirstRunPolicy::Discard => true,
        }
    }

    /// `--aggregate-parameter-runs`: round the run count up to whole cycles
    /// through the parameter values, so that every value is used equally often.
    pub fn whole_cycles(&self, runs: u64) -> u64 {
        runs.div_ceil(self.command.runs_multiple()) * self.command.runs_multiple()
    }

    /// Number of runs to perform in addition to the requested ones, so that
    /// the requested number is still measured when the first one is excluded.
    pub fn extra_runs(&self) -> u64 {
        u64::from(self.excludes_first_timing_run())
    }

    /// `--first-run`: remove the first timing run from the per-run vectors and
    /// return it (or, with warmup, the first warmup run) for `separate`.
    /// A benchmark with a single run keeps it (e.g. after a Ctrl-C).
    fn take_first_run(&mut self) -> Option<FirstRun> {
        if self.excludes_first_timing_run() && self.times_real.len() > 1 {
            let run = FirstRun {
                time: self.times_real[0],
                user: Some(self.times_user[0]),
                system: Some(self.times_system[0]),
                memory_usage_byte: Some(self.memory_usage_byte[0]),
                energy_joules: self.energy_measurements.first().copied().flatten(),
                exit_code: self.exit_codes[0],
                warmup: false,
            };
            let keep: Vec<usize> = (1..self.times_real.len()).collect();
            self.retain_runs(&keep);
            (self.options.first_run == FirstRunPolicy::Separate).then_some(run)
        } else {
            self.cold_warmup_time.map(|time| FirstRun {
                time,
                warmup: true,
                ..Default::default()
            })
        }
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

        let total_time = res.time_real
            + self.executor.time_overhead()
            + preparation_overhead
            + conclusion_overhead;
        self.initial_total_time = total_time;

        let runs_in_min_time = (self.options.min_benchmarking_time / total_time) as u64;

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

        self.count = self.whole_cycles(count) + self.extra_runs();

        self.times_real.push(res.time_real);
        self.times_user.push(res.time_user);
        self.times_system.push(res.time_system);
        self.memory_usage_byte.push(res.memory_usage_byte);
        self.energy_measurements.push(energy_initial);
        self.exit_codes.push(extract_exit_code(status));
        self.resource_counters.push(res.counters);
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
        self.energy_measurements.push(energy);
        self.exit_codes.push(extract_exit_code(status));
        self.resource_counters.push(res.counters);
        self.all_succeeded = self.all_succeeded && success;

        self.run_conclusion(BenchmarkIteration::Benchmark(iteration))?;

        Ok(())
    }

    /// Resource counters of all runs, if every run has them.
    fn all_resource_counters(&self) -> Option<Vec<timing_result::ResourceCounters>> {
        if self.resource_counters.is_empty() {
            return None;
        }
        self.resource_counters.iter().copied().collect()
    }

    /// "ctx-sw 3 vol / 12 invol · faults 1.2k minor / 0 major · I/O 0 in / 0 out blocks"
    fn resource_summary(&self) -> Option<String> {
        let counters = self.all_resource_counters()?;
        let mean_of = |f: fn(&timing_result::ResourceCounters) -> u64| {
            let values: Vec<f64> = counters.iter().map(|c| f(c) as f64).collect();
            format_count(mean(&values))
        };
        Some(format!(
            "ctx-sw {} vol / {} invol · faults {} minor / {} major · I/O {} in / {} out blocks",
            mean_of(|c| c.voluntary_ctx_switches),
            mean_of(|c| c.involuntary_ctx_switches),
            mean_of(|c| c.minor_faults),
            mean_of(|c| c.major_faults),
            mean_of(|c| c.block_input_ops),
            mean_of(|c| c.block_output_ops),
        ))
    }

    /// Keep only the runs at `keep` (ascending indices) in every per-run vector.
    fn retain_runs(&mut self, keep: &[usize]) {
        fn select<T: Copy>(values: &[T], keep: &[usize]) -> Vec<T> {
            keep.iter().map(|&index| values[index]).collect()
        }
        let run_count = self.times_real.len();
        self.times_real = select(&self.times_real, keep);
        self.times_user = select(&self.times_user, keep);
        self.times_system = select(&self.times_system, keep);
        self.memory_usage_byte = select(&self.memory_usage_byte, keep);
        if self.energy_measurements.len() == run_count {
            self.energy_measurements = select(&self.energy_measurements, keep);
        }
        self.exit_codes = select(&self.exit_codes, keep);
        self.resource_counters = select(&self.resource_counters, keep);
    }

    pub fn finish(mut self, print_header: bool) -> Result<BenchmarkResult> {
        let first_run_excluded = self.excludes_first_timing_run() && self.times_real.len() > 1;
        let first_run = self.take_first_run();
        let original_run_count = self.times_real.len();
        let mut omitted_failed_runs = Vec::new();

        if self.options.omit_failed_runs {
            let mut keep_indices = Vec::new();
            for (index, &exit_code) in self.exit_codes.iter().enumerate() {
                if exit_code == Some(0) {
                    keep_indices.push(index);
                } else {
                    omitted_failed_runs.push(OmittedRun { index, exit_code });
                }
            }

            if keep_indices.is_empty() {
                bail!("All benchmark runs failed. No successful runs to compute statistics from.");
            }

            if !omitted_failed_runs.is_empty() {
                self.retain_runs(&keep_indices);
            }
        }

        // Original run numbers of the runs that are still present.
        let run_numbers: Vec<usize> = (0..original_run_count)
            .filter(|index| !omitted_failed_runs.iter().any(|o| o.index == *index))
            .collect();

        let mut discarded_outliers = Vec::new();
        let mut too_many_outliers = false;
        if let Some(threshold) = self.options.discard_outliers {
            match outlier_indices(&self.times_real, threshold, MAX_DISCARD_FRACTION) {
                Some(drop) if !drop.is_empty() => {
                    let keep: Vec<usize> = (0..self.times_real.len())
                        .filter(|index| !drop.contains(index))
                        .collect();
                    discarded_outliers = drop.iter().map(|&index| run_numbers[index]).collect();
                    self.retain_runs(&keep);
                }
                Some(_) => {}
                None => too_many_outliers = true,
            }
        }

        let num_omitted_failed_runs = omitted_failed_runs.len();

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
        let median_str = format_duration(t_median, Some(time_unit));
        let max_str = format_duration(t_max, Some(time_unit));
        let mut excluded = Vec::new();
        if first_run_excluded {
            excluded.push(match self.options.first_run {
                FirstRunPolicy::Discard => "first run discarded".to_string(),
                _ => "+1 cold".to_string(),
            });
        }
        if num_omitted_failed_runs > 0 {
            excluded.push(format!("{num_omitted_failed_runs} failed runs omitted"));
        }
        if !discarded_outliers.is_empty() {
            let n = discarded_outliers.len();
            excluded.push(format!(
                "{n} {} discarded",
                if n == 1 { "outlier" } else { "outliers" }
            ));
        }
        let num_str = if excluded.is_empty() {
            format!("{t_num} runs")
        } else {
            format!("{t_num} runs ({})", excluded.join(", "))
        };

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
            format!(", CPU: {pct:.0}%")
        } else {
            String::new()
        };

        // Runs performed and planned, not counting an excluded first run
        let runs_done = original_run_count;
        let planned = self.count - self.extra_runs();
        let is_interrupted = crate::util::interrupt::interrupted() && (runs_done as u64) < planned;
        let runs_planned = if is_interrupted { Some(planned) } else { None };

        if self.options.output_style != OutputStyleOption::Disabled {
            if print_header {
                let suffix = if is_interrupted {
                    colors::yellow(format!(
                        "  (interrupted after {runs_done} of {planned} runs)"
                    ))
                    .to_string()
                } else {
                    String::new()
                };
                crate::outln!(
                    "{}{}: {}{}",
                    "Benchmark ".bold(),
                    (self.display_number + 1).to_string().bold(),
                    self.command.get_name_with_unused_parameters(),
                    suffix,
                );
            } else if is_interrupted {
                crate::outln!(
                    "  {}",
                    colors::yellow(format!("(interrupted after {runs_done} of {planned} runs)"))
                );
            }

            if let Some(cold) = &first_run {
                // Aligned with the columns of the "Time" line below
                let details = match (cold.user, cold.system) {
                    (Some(user), Some(system)) => format!(
                        "{:15}[User: {}, System: {}]",
                        "",
                        colors::blue(format_duration(user, Some(time_unit))),
                        colors::blue(format_duration(system, Some(time_unit)))
                    ),
                    _ => String::new(),
                };
                crate::outln!(
                    "  {:<21}{:>8}{}",
                    if cold.warmup {
                        "Cold (warmup 1):"
                    } else {
                        "Cold (1st run):"
                    },
                    colors::yellow(format_duration(cold.time, Some(time_unit))).bold(),
                    details
                );
            }

            if let Some(stddev) = t_stddev {
                let stddev_str = format_duration(stddev, Some(time_unit));

                crate::outln!(
                    "  Time ({} ± {}):     {:>8} ± {:>8}    [User: {}, System: {}{}{}]",
                    colors::green("mean").bold(),
                    colors::green("σ"),
                    colors::green(mean_str).bold(),
                    colors::green(stddev_str),
                    colors::blue(user_str),
                    colors::blue(system_str),
                    colors::blue(cpu_str),
                    colors::blue(mem_str)
                );

                crate::outln!(
                    "  Range ({} … {} … {}):   {:>8} … {:>8} … {:>8}    {}",
                    colors::cyan("min"),
                    colors::yellow("median"),
                    colors::purple("max"),
                    colors::cyan(min_str),
                    colors::yellow(median_str),
                    colors::purple(max_str),
                    num_str.dimmed()
                );
            } else {
                let suffix = if !excluded.is_empty() {
                    format!("    {}", num_str.dimmed())
                } else {
                    String::new()
                };
                crate::outln!(
                    "  Time ({} ≡):        {:>8}  {:>8}     [User: {}, System: {}{}{}]{}",
                    colors::green("abs").bold(),
                    colors::green(mean_str).bold(),
                    "        ", // alignment
                    colors::blue(user_str),
                    colors::blue(system_str),
                    colors::blue(cpu_str),
                    colors::blue(mem_str),
                    suffix,
                );
            }

            if let Some(warmup) = self.warmup.filter(|w| w.auto) {
                crate::outln!(
                    "  Warmup (auto):      {}",
                    format!(
                        "{} runs, last {} within {:.1}%{}",
                        warmup.runs,
                        AUTO_WARMUP_WINDOW.min(warmup.runs as usize),
                        warmup.spread * 100.0,
                        if warmup.stable { "" } else { " (not stable)" }
                    )
                    .dimmed()
                );
            }

            if self.options.show_resource_usage {
                match self.resource_summary() {
                    Some(line) => crate::outln!("  Resources (mean):   {}", line.dimmed()),
                    None => crate::outln!(
                        "  Resources:          {}",
                        "not available (Unix only)".dimmed()
                    ),
                }
            }

            if self.options.measure_energy {
                let valid_energy: Vec<f64> =
                    self.energy_measurements.iter().filter_map(|&e| e).collect();
                if !valid_energy.is_empty() {
                    let mean_joules = mean(&valid_energy);
                    let stddev_joules = if valid_energy.len() > 1 {
                        Some(standard_deviation(&valid_energy, Some(mean_joules)))
                    } else {
                        None
                    };
                    let watts = if t_mean > 0.0 {
                        mean_joules / t_mean
                    } else {
                        0.0
                    };

                    let energy_str = if let Some(sd) = stddev_joules {
                        format!("{mean_joules:.3} ± {sd:.3} J")
                    } else {
                        format!("{mean_joules:.3} J")
                    };

                    crate::outln!(
                        "  Energy ({}):        {:>14}    [Power: {}]",
                        colors::yellow("mean").bold(),
                        colors::yellow(energy_str).bold(),
                        colors::yellow(format!("{watts:.2} W"))
                    );
                } else {
                    crate::outln!(
                        "  Energy:             {:>14}",
                        "RAPL unprivileged/unavailable on host".dimmed()
                    );
                }
            }

            if self.options.deep_stats {
                if let Some(deep) = compute_deep_stats(&self.times_real) {
                    crate::outln!(
                        "  Bootstrap 95% CI:   [mean: {} … {}, median: {} … {}, σ: {} … {}]",
                        format_duration(deep.mean_ci_lower, Some(time_unit)),
                        format_duration(deep.mean_ci_upper, Some(time_unit)),
                        format_duration(deep.median_ci_lower, Some(time_unit)),
                        format_duration(deep.median_ci_upper, Some(time_unit)),
                        format_duration(deep.std_dev_ci_lower, Some(time_unit)),
                        format_duration(deep.std_dev_ci_upper, Some(time_unit))
                    );
                }
                if let Some([p05, p25, p75, p95]) =
                    crate::stats::summary::quartiles_and_tails(&self.times_real)
                {
                    let fmt = |v| format_duration(v, Some(time_unit));
                    let geomean_str = crate::stats::summary::geometric_mean(&self.times_real)
                        .map(|g| format!(", geometric mean: {}", fmt(g)))
                        .unwrap_or_default();
                    crate::outln!(
                        "  Percentiles:        [p05: {}, p25: {}, p75: {}, p95: {} (IQR {}){}]",
                        fmt(p05),
                        fmt(p25),
                        fmt(p75),
                        fmt(p95),
                        fmt(p75 - p25),
                        geomean_str
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

        if let Some(warmup) = self.warmup.filter(|w| w.auto && !w.stable) {
            warnings.push(Warnings::WarmupNotStable {
                runs: warmup.runs,
                spread: warmup.spread,
            });
        }
        if too_many_outliers {
            warnings.push(Warnings::TooManyOutliers {
                total: self.times_real.len(),
            });
        }
        if num_omitted_failed_runs > 0 {
            warnings.push(Warnings::FailedRunsOmitted {
                omitted: num_omitted_failed_runs,
                total: original_run_count,
            });
        }

        // Run outlier detection
        let scores = modified_zscores(&self.times_real);

        let outlier_warning_options = OutlierWarningOptions {
            warmup_in_use: self.options.warmup_count > 0 || self.options.warmup_auto,
            prepare_in_use: self
                .options
                .preparation_command
                .as_ref()
                .map(|v| v.len())
                .unwrap_or(0)
                > 0,
        };

        if !self.options.suppress_off_cpu_warnings {
            let total_cpu = user_mean + system_mean;
            if t_mean >= 0.1 && (total_cpu <= 0.0 || t_mean >= 5.0 * total_cpu) {
                let ratio = if total_cpu > 0.0 {
                    t_mean / total_cpu
                } else {
                    f64::INFINITY
                };
                warnings.push(Warnings::OffCpuTime(t_mean, total_cpu, ratio));
            }
        }

        let diagnostics = Diagnostics::compute(&self.times_real, OUTLIER_THRESHOLD);

        if !self.options.suppress_outlier_warnings {
            if scores[0] > OUTLIER_THRESHOLD {
                warnings.push(Warnings::SlowInitialRun(
                    self.times_real[0],
                    outlier_warning_options,
                ));
            } else if diagnostics.has_inflated_variance() && !diagnostics.is_multimodal() {
                // More specific than the generic outlier warning below
                warnings.push(Warnings::InflatedVariance(
                    diagnostics.outlier_variance_fraction.unwrap_or_default(),
                    diagnostics.outlier_count.unwrap_or_default(),
                ));
            } else if scores.iter().any(|&s| s.abs() > OUTLIER_THRESHOLD) {
                warnings.push(Warnings::OutliersDetected(outlier_warning_options));
            }
            if diagnostics.has_trend() {
                warnings.push(Warnings::Trend(
                    diagnostics.trend_rel.unwrap_or_default(),
                    diagnostics.trend_p.unwrap_or(1.0),
                ));
            }
            if diagnostics.is_multimodal() {
                warnings.push(Warnings::Multimodal(
                    diagnostics.bimodality.unwrap_or_default(),
                ));
            }
        }

        if !warnings.is_empty() {
            eprintln!(" ");

            for warning in &warnings {
                eprintln!("  {}: {}", colors::yellow("Warning"), warning);
            }
        }

        if self.options.output_style != OutputStyleOption::Disabled {
            crate::outln!(" ");
        }

        let resources = if self.options.show_resource_usage {
            self.all_resource_counters()
                .map(|c| timing_result::ResourceSeries::from_counters(&c))
        } else {
            None
        };

        let total_runs = self.times_real.len();
        let valid_energy: Vec<f64> = self.energy_measurements.into_iter().flatten().collect();
        let (mean_energy, mean_watts, energy_all) = if !valid_energy.is_empty() {
            let m_j = mean(&valid_energy);
            let m_w = if t_mean > 0.0 { m_j / t_mean } else { 0.0 };
            let energy_all = if valid_energy.len() == total_runs {
                Some(valid_energy)
            } else {
                None
            };
            (Some(m_j), Some(m_w), energy_all)
        } else {
            (None, None, None)
        };

        if let Some(sampler) = self.energy_sampler.as_mut() {
            let _ = sampler.stop();
        }

        Ok(BenchmarkResult {
            command: self.command.get_name(),
            command_with_unused_parameters: self.command.get_name_with_unused_parameters(),
            mean: t_mean,
            stddev: t_stddev,
            median: t_median,
            percentiles: crate::stats::summary::quartiles_and_tails(&self.times_real)
                .map(|[p05, p25, p75, p95]| benchmark_result::Percentiles { p05, p25, p75, p95 }),
            geometric_mean: crate::stats::summary::geometric_mean(&self.times_real),
            warmup_runs: self.warmup.filter(|w| w.auto).map(|w| w.runs),
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
            omitted_failed_runs,
            discarded_outliers,
            resources,
            runs_planned,
            first_run,
            diagnostics: (!diagnostics.is_empty()).then_some(diagnostics),
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
    pub fn run(&self) -> Result<Option<BenchmarkResult>> {
        let mut runner = BenchmarkRunner::new(
            self.number,
            self.display_number,
            self.command,
            self.options,
            self.executor,
        );

        if self.options.output_style != OutputStyleOption::Disabled {
            crate::outln!(
                "{}{}: {}",
                "Benchmark ".bold(),
                (self.display_number + 1).to_string().bold(),
                self.command.get_name_with_unused_parameters(),
            );
        }

        if crate::util::interrupt::interrupted() {
            if self.options.output_style != OutputStyleOption::Disabled {
                crate::outln!(
                    "  {}",
                    colors::yellow("(interrupted before completing any runs)")
                );
            }
            return Ok(None);
        }

        runner.run_setup()?;

        // Warmup phase
        if self.options.warmup_auto {
            let progress_bar = if self.options.output_style != OutputStyleOption::Disabled {
                Some(get_progress_bar(
                    AUTO_WARMUP_MAX_RUNS,
                    "Performing warmup runs (auto)",
                    self.options.output_style,
                ))
            } else {
                None
            };
            let warmup_res = runner.run_auto_warmup(|| {
                if let Some(bar) = progress_bar.as_ref() {
                    bar.inc(1)
                }
            });
            if let Some(bar) = progress_bar.as_ref() {
                bar.finish_and_clear()
            }
            match warmup_res {
                Ok(w) => runner.warmup = Some(w),
                Err(e) if e.is::<crate::error::Interrupted>() => {
                    let _ = runner.run_cleanup();
                    if self.options.output_style != OutputStyleOption::Disabled {
                        crate::outln!(
                            "  {}",
                            colors::yellow("(interrupted before completing any runs)")
                        );
                    }
                    return Ok(None);
                }
                Err(e) => return Err(e),
            }
        } else if self.options.warmup_count > 0 {
            let progress_bar = if self.options.output_style != OutputStyleOption::Disabled {
                Some(get_progress_bar(
                    self.options.warmup_count,
                    if self.options.warmup_count == 1 {
                        "Performing warmup run"
                    } else {
                        "Performing warmup runs"
                    },
                    self.options.output_style,
                ))
            } else {
                None
            };

            for i in 0..self.options.warmup_count {
                if crate::util::interrupt::interrupted() {
                    break;
                }
                match runner.run_warmup_iteration(i) {
                    Ok(_) => {
                        if let Some(bar) = progress_bar.as_ref() {
                            bar.inc(1)
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
                bar.finish_and_clear()
            }
            if crate::util::interrupt::interrupted() {
                let _ = runner.run_cleanup();
                if self.options.output_style != OutputStyleOption::Disabled {
                    crate::outln!(
                        "  {}",
                        colors::yellow("(interrupted before completing any runs)")
                    );
                }
                return Ok(None);
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

        if crate::util::interrupt::interrupted() {
            if let Some(bar) = progress_bar.as_ref() {
                bar.finish_and_clear();
            }
            let _ = runner.run_cleanup();
            if self.options.output_style != OutputStyleOption::Disabled {
                crate::outln!(
                    "  {}",
                    colors::yellow("(interrupted before completing any runs)")
                );
            }
            return Ok(None);
        }

        match runner.run_initial_measurement() {
            Ok(()) => {}
            Err(e) if e.is::<crate::error::Interrupted>() => {
                if let Some(bar) = progress_bar.as_ref() {
                    bar.finish_and_clear();
                }
                let _ = runner.run_cleanup();
                if self.options.output_style != OutputStyleOption::Disabled {
                    crate::outln!(
                        "  {}",
                        colors::yellow("(interrupted before completing any runs)")
                    );
                }
                return Ok(None);
            }
            Err(e) => {
                if let Some(bar) = progress_bar.as_ref() {
                    bar.finish_and_clear();
                }
                return Err(e);
            }
        }

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
            if crate::util::interrupt::interrupted() {
                break;
            }

            let msg = {
                let mean = format_duration(mean(&runner.times_real), self.options.time_unit);
                format!("Current estimate: {}", colors::green(mean.to_string()))
            };

            if let Some(bar) = progress_bar.as_ref() {
                bar.set_message(msg.to_owned())
            }

            match runner.run_timed_iteration(i + 1) {
                Ok(()) => {
                    if let Some(bar) = progress_bar.as_ref() {
                        bar.inc(1)
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
            bar.finish_and_clear()
        }

        if crate::util::interrupt::interrupted() {
            let _ = runner.run_cleanup();
        } else {
            runner.run_cleanup()?;
        }

        if runner.times_real.is_empty() {
            if self.options.output_style != OutputStyleOption::Disabled {
                crate::outln!(
                    "  {}",
                    colors::yellow("(interrupted before completing any runs)")
                );
            }
            Ok(None)
        } else {
            runner.finish(false).map(Some)
        }
    }
}

/// Compact count for the resource line: 3, 12.5, 1.2k, 3.4M.
fn format_count(value: f64) -> String {
    if value >= 1e6 {
        format!("{:.1}M", value / 1e6)
    } else if value >= 1e3 {
        format!("{:.1}k", value / 1e3)
    } else if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[test]
fn test_format_count() {
    assert_eq!(format_count(0.0), "0");
    assert_eq!(format_count(3.0), "3");
    assert_eq!(format_count(12.5), "12.5");
    assert_eq!(format_count(1234.0), "1.2k");
    assert_eq!(format_count(3_400_000.0), "3.4M");
}

/// The value for benchmark `number` of an option that is given either once
/// (for all commands) or once per command, like --prepare or --setup.
fn per_command(values: &[String], number: usize) -> &str {
    if values.len() == 1 {
        &values[0]
    } else {
        &values[number]
    }
}

/// Number of most recent warmup runs that must be stable for `--warmup auto`
pub const AUTO_WARMUP_WINDOW: usize = 5;
/// Largest relative spread `(max - min) / median` of that window
pub const AUTO_WARMUP_THRESHOLD: f64 = 0.01;
/// Upper bound on the number of warmup runs for `--warmup auto`
pub const AUTO_WARMUP_MAX_RUNS: u64 = 100;

/// Outcome of the warmup phase of one benchmark.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WarmupSummary {
    pub runs: u64,
    pub auto: bool,
    pub stable: bool,
    /// Relative spread of the last `AUTO_WARMUP_WINDOW` warmup runs
    pub spread: f64,
}

/// `(max - min) / median` of the given timings (0 for an empty slice).
fn relative_spread(times: &[f64]) -> f64 {
    if times.is_empty() {
        return 0.0;
    }
    let median = median(times);
    if median <= 0.0 {
        return 0.0;
    }
    (max(times) - min(times)) / median
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::executor::MockExecutor;

    #[test]
    fn test_relative_spread() {
        assert_eq!(relative_spread(&[]), 0.0);
        assert_eq!(relative_spread(&[1.0, 1.0, 1.0]), 0.0);
        assert!((relative_spread(&[0.99, 1.0, 1.01]) - 0.02).abs() < 1e-12);
        assert!((relative_spread(&[1.0, 2.0, 3.0]) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_energy_vector_dropped_when_incomplete() {
        let cmd = Command::new(None, "true");
        let options = Options {
            command_output_policies: vec![CommandOutputPolicy::Null],
            ..Default::default()
        };
        let executor = MockExecutor::new(None);
        let mut runner = BenchmarkRunner::new(0, 0, &cmd, &options, &executor);
        runner.times_real = vec![0.1, 0.2];
        runner.times_user = vec![0.05, 0.05];
        runner.times_system = vec![0.05, 0.05];
        runner.memory_usage_byte = vec![100, 100];
        runner.energy_measurements = vec![Some(1.5), None]; // one measurement missing
        runner.exit_codes = vec![Some(0), Some(0)];
        runner.resource_counters = vec![None, None];
        runner.all_succeeded = true;
        runner.count = 2;

        let result = runner.finish(false).unwrap();
        // Since one sample is missing, per-run energy_joules is dropped to prevent misalignment
        assert_eq!(result.energy_joules, None);
        // But aggregate mean energy is still computed from the valid samples
        assert_eq!(result.mean_energy_joules, Some(1.5));
    }

    #[test]
    fn test_energy_vector_preserved_when_complete() {
        let cmd = Command::new(None, "true");
        let options = Options {
            command_output_policies: vec![CommandOutputPolicy::Null],
            ..Default::default()
        };
        let executor = MockExecutor::new(None);
        let mut runner = BenchmarkRunner::new(0, 0, &cmd, &options, &executor);
        runner.times_real = vec![0.1, 0.2];
        runner.times_user = vec![0.05, 0.05];
        runner.times_system = vec![0.05, 0.05];
        runner.memory_usage_byte = vec![100, 100];
        runner.energy_measurements = vec![Some(1.5), Some(2.5)]; // all valid
        runner.exit_codes = vec![Some(0), Some(0)];
        runner.resource_counters = vec![None, None];
        runner.all_succeeded = true;
        runner.count = 2;

        let result = runner.finish(false).unwrap();
        assert_eq!(result.energy_joules, Some(vec![1.5, 2.5]));
        assert_eq!(result.mean_energy_joules, Some(2.0));
    }
}
