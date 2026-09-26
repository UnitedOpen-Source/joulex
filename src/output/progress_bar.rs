use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use std::fmt::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::options::OutputStyleOption;
use crate::output::colors;
use crate::output::format::{format_duration, format_duration_unit};
use crate::stats::basic::{mean, standard_deviation};
use crate::util::units::{Second, Unit};

#[cfg(not(windows))]
const TICK_SETTINGS: (&str, u64) = ("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ", 80);

#[cfg(windows)]
const TICK_SETTINGS: (&str, u64) = (r"+-x| ", 200);

pub const DEFAULT_MESSAGE_TEMPLATE: &str = "{msg:<32}";
pub const INITIAL_MEASUREMENT_TEMPLATE: &str = " {spinner} {msg} {elapsed_precise}";

/// Format a progress bar template containing the given message template.
pub fn create_progress_template(msg_template: &str) -> String {
    format!(
        " {{spinner}} {} {{wide_bar}} {{pos}}/{{len}} ETA {{joulex_eta}} ",
        msg_template
    )
}

/// Replace the usual `message` in a progress bar with the result of evaluating `template`.
/// Replace the usual `message` in a progress bar with the result of evaluating `template`.
/// The `template` may contain a `ProgressBar`'s templated fields.
#[must_use]
pub fn replace_message_template(bar: ProgressBar, template: &str) -> ProgressBar {
    let old_style = bar.style();
    let new_template = create_progress_template(template);
    match old_style.template(&new_template) {
        Ok(new_style) => bar.with_style(new_style),
        Err(_) => bar,
    }
}

/// Reset a progress bar's template to the default.
/// This is useful after calling `replace_message_template()` or `set_initial_measurement_template()`.
#[must_use]
pub fn reset_progress_template(bar: ProgressBar) -> ProgressBar {
    replace_message_template(bar, DEFAULT_MESSAGE_TEMPLATE)
}

/// Configure the progress bar for the initial time measurement phase with a ticking elapsed time.
#[must_use]
pub fn set_initial_measurement_template(bar: ProgressBar) -> ProgressBar {
    let old_style = bar.style();
    match old_style.template(INITIAL_MEASUREMENT_TEMPLATE) {
        Ok(new_style) => bar.with_style(new_style),
        Err(_) => bar,
    }
}

/// Format the live progress estimate message displayed in the progress bar.
///
/// Formats:
/// - Single run: `Current estimate: <mean>`
/// - Multiple runs: `Current estimate: <mean> ± <stddev>`
/// - With energy (single): `Current estimate: <mean> · <energy> J`
/// - With energy (multiple): `Current estimate: <mean> ± <stddev> · <energy> J ± <energy_sd> J`
pub fn format_progress_estimate(
    times_real: &[Second],
    time_unit: Option<Unit>,
    energy_measurements: Option<&[Option<f64>]>,
) -> String {
    if times_real.is_empty() {
        let mean_str = format_duration(0.0, time_unit);
        return format!("Current estimate: {}", colors::green(mean_str));
    }

    let mean_time = mean(times_real);
    let (mean_str, unit) = format_duration_unit(mean_time, time_unit);

    let mut msg = if times_real.len() > 1 {
        let stddev = standard_deviation(times_real, Some(mean_time));
        let stddev_str = format_duration(stddev, Some(unit));
        format!(
            "Current estimate: {} ± {}",
            colors::green(mean_str),
            colors::cyan(stddev_str)
        )
    } else {
        format!("Current estimate: {}", colors::green(mean_str))
    };

    if let Some(energies) = energy_measurements {
        let valid_energy: Vec<f64> = energies.iter().filter_map(|&e| e).collect();
        if !valid_energy.is_empty() {
            let mean_joules = mean(&valid_energy);
            let energy_part = if valid_energy.len() > 1 {
                let sd_joules = standard_deviation(&valid_energy, Some(mean_joules));
                format!("{mean_joules:.3} J ± {sd_joules:.3} J")
            } else {
                format!("{mean_joules:.3} J")
            };
            msg.push_str(&format!(" · {}", colors::yellow(energy_part)));
        }
    }

    msg
}

/// Estimated time until all `total` steps are done.
///
/// `done_at` is the elapsed time when the `done`-th step finished. The average
/// step duration up to then, times the remaining steps, minus the time already
/// spent on the current step, gives an estimate that decreases steadily while
/// a step is running instead of growing. None while no step has finished yet.
///
/// joulex computes this itself instead of using indicatif's `{eta}`: since
/// indicatif 0.17.5 its estimate grows instead of shrinking when every step
/// takes long, e.g. one benchmark run of several seconds (hyperfine#670).
fn eta(elapsed: Duration, done: u64, done_at: Duration, total: u64) -> Option<Duration> {
    if done == 0 {
        return None;
    }
    let remaining = total.saturating_sub(done) as f64;
    let per_step = done_at.as_secs_f64() / done as f64;
    let in_current_step = elapsed.saturating_sub(done_at).as_secs_f64();
    Some(Duration::from_secs_f64(
        (per_step * remaining - in_current_step).max(0.0),
    ))
}

fn format_eta(eta: Option<Duration>) -> String {
    match eta {
        None => "--:--:--".into(),
        Some(eta) => {
            let secs = eta.as_secs();
            format!(
                "{:02}:{:02}:{:02}",
                secs / 3600,
                (secs / 60) % 60,
                secs % 60
            )
        }
    }
}

/// Return a pre-configured progress bar
pub fn get_progress_bar(length: u64, msg: &str, option: OutputStyleOption) -> ProgressBar {
    let template = create_progress_template(DEFAULT_MESSAGE_TEMPLATE);
    let progressbar_style = match option {
        OutputStyleOption::Basic | OutputStyleOption::Color => ProgressStyle::default_bar(),
        _ => ProgressStyle::default_spinner()
            .tick_chars(TICK_SETTINGS.0)
            .template(&template)
            .expect("no template error")
            .with_key("joulex_eta", {
                // (position, elapsed time when that position was reached), shared by
                // all clones of this formatter (indicatif requires it to be Clone)
                let last_step = Arc::new(Mutex::new((0u64, Duration::ZERO)));
                move |state: &ProgressState, w: &mut dyn Write| {
                    let mut last = last_step.lock().unwrap_or_else(|e| e.into_inner());
                    if state.pos() != last.0 {
                        *last = (state.pos(), state.elapsed());
                    }
                    let estimate = eta(state.elapsed(), last.0, last.1, state.len().unwrap_or(0));
                    let _ = w.write_str(&format_eta(estimate));
                }
            }),
    };

    let progress_bar = match option {
        OutputStyleOption::Basic | OutputStyleOption::Color => ProgressBar::hidden(),
        _ => ProgressBar::new(length),
    };
    progress_bar.set_style(progressbar_style);
    progress_bar.enable_steady_tick(Duration::from_millis(TICK_SETTINGS.1));
    progress_bar.set_message(msg.to_owned());

    progress_bar
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn eta_is_unknown_before_the_first_step() {
        assert_eq!(eta(secs(5), 0, Duration::ZERO, 10), None);
        assert_eq!(format_eta(None), "--:--:--");
    }

    #[test]
    fn eta_uses_the_average_step_duration() {
        // 2 of 10 steps done after 10 s: 5 s per step, 8 steps left
        assert_eq!(eta(secs(10), 2, secs(10), 10), Some(secs(40)));
        assert_eq!(format_eta(Some(secs(40))), "00:00:40");
        assert_eq!(format_eta(Some(secs(3725))), "01:02:05");
    }

    #[test]
    fn eta_decreases_while_a_step_is_running() {
        // 1 of 4 steps done after 5 s; then time passes without progress.
        let estimates: Vec<Duration> = (5..=9)
            .map(|now| eta(secs(now), 1, secs(5), 4).unwrap())
            .collect();
        assert_eq!(estimates.first(), Some(&secs(15)));
        assert!(estimates.windows(2).all(|w| w[1] < w[0]), "{estimates:?}");
    }

    #[test]
    fn eta_never_goes_negative_or_underflows() {
        // current step takes longer than the average
        assert_eq!(eta(secs(100), 1, secs(5), 2), Some(Duration::ZERO));
        // done exceeds total
        assert_eq!(eta(secs(3), 5, secs(3), 3), Some(Duration::ZERO));
    }

    #[test]
    fn test_create_progress_template() {
        let t = create_progress_template("{msg:<32}");
        assert_eq!(
            t,
            " {spinner} {msg:<32} {wide_bar} {pos}/{len} ETA {joulex_eta} "
        );
    }

    #[test]
    fn test_replace_and_reset_progress_template() {
        let bar = get_progress_bar(10, "testing", OutputStyleOption::Full);
        let bar = replace_message_template(bar, "{msg} {elapsed_precise}");
        assert_eq!(bar.length(), Some(10));
        assert_eq!(bar.message(), "testing");
        let bar = reset_progress_template(bar);
        assert_eq!(bar.length(), Some(10));
    }

    #[test]
    fn test_set_initial_measurement_template() {
        let bar = get_progress_bar(10, "Initial time measurement", OutputStyleOption::Full);
        let bar = set_initial_measurement_template(bar);
        assert_eq!(bar.message(), "Initial time measurement");
        let bar = reset_progress_template(bar);
        assert_eq!(bar.length(), Some(10));
    }

    #[test]
    fn test_hidden_bar_safe_for_all_template_operations() {
        let bar = get_progress_bar(10, "hidden", OutputStyleOption::Basic);
        assert!(bar.is_hidden());
        let bar = set_initial_measurement_template(bar);
        assert!(bar.is_hidden());
        let bar = replace_message_template(bar, "{msg}");
        assert!(bar.is_hidden());
        let bar = reset_progress_template(bar);
        assert!(bar.is_hidden());
    }

    #[test]
    fn test_format_progress_estimate_single_run() {
        colored::control::set_override(false);
        let times = vec![0.025]; // 25 ms
        let msg = format_progress_estimate(&times, None, None);
        assert_eq!(msg, "Current estimate: 25.0 ms");
    }

    #[test]
    fn test_format_progress_estimate_multiple_runs() {
        colored::control::set_override(false);
        let times = vec![0.020, 0.030]; // mean 25 ms
        let msg = format_progress_estimate(&times, None, None);
        assert!(msg.starts_with("Current estimate: 25.0 ms ± "));
    }

    #[test]
    fn test_format_progress_estimate_with_energy_single() {
        colored::control::set_override(false);
        let times = vec![0.025];
        let energy = vec![Some(1.234)];
        let msg = format_progress_estimate(&times, None, Some(&energy));
        assert_eq!(msg, "Current estimate: 25.0 ms · 1.234 J");
    }

    #[test]
    fn test_format_progress_estimate_with_energy_multiple() {
        colored::control::set_override(false);
        let times = vec![0.020, 0.030];
        let energy = vec![Some(1.200), Some(1.400)];
        let msg = format_progress_estimate(&times, None, Some(&energy));
        assert!(msg.starts_with("Current estimate: 25.0 ms ± "));
        assert!(msg.contains("· 1.300 J ± 0.141 J"));
    }

    #[test]
    fn test_format_progress_estimate_empty_and_none_energy() {
        colored::control::set_override(false);
        let empty_times: Vec<f64> = vec![];
        let msg = format_progress_estimate(&empty_times, None, None);
        assert_eq!(msg, "Current estimate: 0.0 µs");

        let msg_ms = format_progress_estimate(&empty_times, Some(Unit::MilliSecond), None);
        assert_eq!(msg_ms, "Current estimate: 0.0 ms");

        let times = vec![0.010];
        let none_energy = vec![None, None];
        let msg = format_progress_estimate(&times, None, Some(&none_energy));
        assert_eq!(msg, "Current estimate: 10.0 ms");
    }
}
