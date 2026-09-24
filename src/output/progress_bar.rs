use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use std::fmt::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::options::OutputStyleOption;

#[cfg(not(windows))]
const TICK_SETTINGS: (&str, u64) = ("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ", 80);

#[cfg(windows)]
const TICK_SETTINGS: (&str, u64) = (r"+-x| ", 200);

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
    let progressbar_style = match option {
        OutputStyleOption::Basic | OutputStyleOption::Color => ProgressStyle::default_bar(),
        _ => ProgressStyle::default_spinner()
            .tick_chars(TICK_SETTINGS.0)
            .template(" {spinner} {msg:<30} {wide_bar} ETA {joulex_eta} ")
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
}
