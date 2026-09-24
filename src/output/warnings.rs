use std::fmt;

use crate::benchmark::MIN_EXECUTION_TIME;
use crate::output::format::format_duration;
use crate::util::units::Second;

pub struct OutlierWarningOptions {
    pub warmup_in_use: bool,
    pub prepare_in_use: bool,
}

/// A list of all possible warnings
pub enum Warnings {
    FastExecutionTime,
    NonZeroExitCode,
    SlowInitialRun(Second, OutlierWarningOptions),
    OutliersDetected(OutlierWarningOptions),
    OffCpuTime(Second, Second, f64),
    FailedRunsOmitted { omitted: usize, total: usize },
    TooManyOutliers { total: usize },
}

impl fmt::Display for Warnings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Warnings::FastExecutionTime => write!(
                f,
                "Command took less than {:.0} ms to complete. Note that the results might be \
                inaccurate because joulex can not calibrate the shell startup time much \
                more precise than this limit. You can try to use the `-N`/`--shell=none` \
                option to disable the shell completely.",
                MIN_EXECUTION_TIME * 1e3
            ),
            Warnings::NonZeroExitCode => write!(f, "Ignoring non-zero exit code."),
            Warnings::SlowInitialRun(time_first_run, ref options) => write!(
                f,
                "The first benchmarking run for this command was significantly slower than the \
                 rest ({time}). This could be caused by (filesystem) caches that were not filled until \
                 after the first run. {hints}",
                time=format_duration(time_first_run, None),
                hints=match (options.warmup_in_use, options.prepare_in_use) {
                    (true, true) => "You are already using both the '--warmup' option as well \
                    as the '--prepare' option. Consider re-running the benchmark on a quiet system. \
                    Maybe it was a random outlier. Alternatively, consider increasing the warmup \
                    count.",
                    (true, false) => "You are already using the '--warmup' option which helps \
                    to fill these caches before the actual benchmark. You can either try to \
                    increase the warmup count further or re-run this benchmark on a quiet system \
                    in case it was a random outlier. Alternatively, consider using the '--prepare' \
                    option to clear the caches before each timing run.",
                    (false, true) => "You are already using the '--prepare' option which can \
                    be used to clear caches. If you did not use a cache-clearing command with \
                    '--prepare', you can either try that or consider using the '--warmup' option \
                    to fill those caches before the actual benchmark.",
                    (false, false) => "You should consider using the '--warmup' option to fill \
                    those caches before the actual benchmark. Alternatively, use the '--prepare' \
                    option to clear the caches before each timing run."
                }
            ),
            Warnings::OutliersDetected(ref options) => write!(
                f,
                "Statistical outliers were detected. Consider re-running this benchmark on a quiet \
                 system without any interferences from other programs.{hint}",
                hint=if options.warmup_in_use && options.prepare_in_use {
                    ""
                } else {
                    " It might help to use the '--warmup' or '--prepare' options."
                }
            ),
            Warnings::OffCpuTime(wall, cpu, ratio) => {
                let ratio_str = if ratio.is_infinite() {
                    "∞".to_string()
                } else {
                    format!("{ratio:.1}x")
                };
                write!(
                    f,
                    "Substantial off-CPU time detected: the process spent most of its wall-clock time \
                     waiting (I/O, sleep, locks, or system scheduling) rather than executing on-CPU \
                     (wall: {wall_str}, CPU: {cpu_str}, ratio: {ratio_str}).",
                    wall_str = format_duration(wall, None),
                    cpu_str = format_duration(cpu, None),
                    ratio_str = ratio_str,
                )
            }
            Warnings::TooManyOutliers { total } => write!(
                f,
                "Too many of the {total} runs (more than {:.0}%) look like outliers, so none were discarded \
                 ('--discard-outliers'). The run time distribution is probably multimodal \
                 (e.g. caching effects or a background process); inspect the individual run times.",
                crate::outlier_detection::MAX_DISCARD_FRACTION * 100.0
            ),
            Warnings::FailedRunsOmitted { omitted, total } => write!(
                f,
                "Omitted {omitted} of {total} benchmark runs with non-zero exit codes from the \
                 summary statistics."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_off_cpu_time_warning_format() {
        let warning = Warnings::OffCpuTime(1.0, 0.002, 500.0);
        let msg = format!("{warning}");
        assert!(msg.contains("Substantial off-CPU time detected"));
        assert!(msg.contains("500.0x"));

        let warning_inf = Warnings::OffCpuTime(1.0, 0.0, f64::INFINITY);
        let msg_inf = format!("{warning_inf}");
        assert!(msg_inf.contains("∞"));
    }

    #[test]
    fn test_failed_runs_omitted_warning_format() {
        let warning = Warnings::FailedRunsOmitted {
            omitted: 3,
            total: 10,
        };
        let msg = format!("{warning}");
        assert_eq!(
            msg,
            "Omitted 3 of 10 benchmark runs with non-zero exit codes from the summary statistics."
        );
    }
}
