//! `--compare baseline.json`: compare each benchmark with the result of the
//! same command in a baseline JSON export, and (`--fail-if-regressed`) fail
//! when a benchmark got significantly slower. Meant as a CI regression gate.

use std::collections::HashMap;

use crate::benchmark::benchmark_result::BenchmarkResult;
use crate::output::format::format_duration_unit;
use crate::util::units::Unit;

/// Significance level for a regression
pub const MAX_P_VALUE: f64 = 0.05;

/// How a current result compares with its baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct Delta<'a> {
    pub name: &'a str,
    pub baseline: &'a BenchmarkResult,
    pub current: &'a BenchmarkResult,
    /// current / baseline − 1 (0.13 = 13% slower)
    pub rel_change: f64,
    /// p-value of the difference, if both sides have enough per-run times
    pub p_value: Option<f64>,
}

impl Delta<'_> {
    pub fn is_significant(&self) -> bool {
        self.p_value.is_some_and(|p| p < MAX_P_VALUE)
    }

    /// Slower by more than `threshold` (0.05 = 5%) and significant.
    pub fn is_regression(&self, threshold: f64) -> bool {
        self.rel_change > threshold && self.is_significant()
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Comparison<'a> {
    pub deltas: Vec<Delta<'a>>,
    /// Commands without a baseline
    pub new: Vec<&'a str>,
    /// Baseline commands that were not benchmarked now
    pub removed: Vec<&'a str>,
}

impl Comparison<'_> {
    pub fn regressions(&self, threshold: f64) -> impl Iterator<Item = &Delta<'_>> {
        self.deltas
            .iter()
            .filter(move |d| d.is_regression(threshold))
    }
}

/// The name that matches a result with its baseline: the displayed command
/// name (after parameter substitution, or the `-n` name).
fn key(result: &BenchmarkResult) -> &str {
    &result.command
}

pub fn compare<'a>(
    baseline: &'a [BenchmarkResult],
    current: &'a [BenchmarkResult],
) -> Comparison<'a> {
    let base_by_name: HashMap<&str, &BenchmarkResult> =
        baseline.iter().map(|r| (key(r), r)).collect();
    let current_names: std::collections::HashSet<&str> = current.iter().map(key).collect();

    let mut comparison = Comparison::default();
    for cur in current {
        match base_by_name.get(key(cur)) {
            Some(&base) => comparison.deltas.push(Delta {
                name: key(cur),
                baseline: base,
                current: cur,
                rel_change: cur.mean / base.mean - 1.0,
                p_value: p_value(base, cur),
            }),
            None => comparison.new.push(key(cur)),
        }
    }
    comparison.removed = baseline
        .iter()
        .map(key)
        .filter(|name| !current_names.contains(name))
        .collect();
    comparison
}

/// Two-sided p-value of the difference in mean run time (bootstrap Welch
/// test). Two constant samples with different means are certainly different.
fn p_value(baseline: &BenchmarkResult, current: &BenchmarkResult) -> Option<f64> {
    let (a, b) = (baseline.times.as_deref()?, current.times.as_deref()?);
    if let Some(stats) = crate::stats::deep::compare_samples(a, b) {
        return Some(stats.p_value);
    }
    let constant = |xs: &[f64]| xs.len() >= 3 && xs.iter().all(|&x| x == xs[0]);
    (constant(a) && constant(b) && a[0] != b[0]).then_some(0.0)
}

/// Parse `--fail-if-regressed`: "5%", "5" or "0.5%" → 0.05, 0.05, 0.005.
pub fn parse_threshold(value: &str) -> Result<f64, String> {
    let number = value.trim().trim_end_matches('%').trim();
    match number.parse::<f64>() {
        Ok(pct) if pct.is_finite() && pct >= 0.0 => Ok(pct / 100.0),
        _ => Err(format!(
            "invalid threshold '{value}': expected a non-negative percentage such as '5%'"
        )),
    }
}

fn format_time(result: &BenchmarkResult, unit: Option<Unit>) -> String {
    let (mean, unit) = format_duration_unit(result.mean, unit);
    match result.stddev {
        Some(stddev) => format!(
            "{mean} ± {}",
            crate::output::format::format_duration_value(stddev, Some(unit)).0
        ),
        None => mean,
    }
}

fn format_p(p: Option<f64>) -> String {
    match p {
        None => "n/a".to_string(),
        Some(p) if p < 0.001 => "p < 0.001".to_string(),
        Some(p) => format!("p = {p:.3}"),
    }
}

/// "REGRESSION", "slower", "faster" or "~" (no significant difference).
fn verdict(delta: &Delta<'_>, threshold: Option<f64>) -> &'static str {
    if threshold.is_some_and(|t| delta.is_regression(t)) {
        "REGRESSION"
    } else if !delta.is_significant() {
        "~"
    } else if delta.rel_change > 0.0 {
        "slower"
    } else {
        "faster"
    }
}

/// One row per compared command: name, baseline, current, change, p, verdict.
fn rows(
    comparison: &Comparison<'_>,
    threshold: Option<f64>,
    unit: Option<Unit>,
) -> Vec<[String; 6]> {
    let mut rows: Vec<[String; 6]> = comparison
        .deltas
        .iter()
        .map(|d| {
            let arrow = match (d.is_significant(), d.rel_change > 0.0) {
                (false, _) => "",
                (true, true) => " ▲",
                (true, false) => " ▼",
            };
            [
                crate::util::sanitize::escape_control_chars(d.name).into_owned(),
                format_time(d.baseline, unit),
                format_time(d.current, unit),
                format!("{:+.1}%{arrow}", d.rel_change * 100.0),
                format_p(d.p_value),
                verdict(d, threshold).to_string(),
            ]
        })
        .collect();
    for (names, label) in [(&comparison.new, "new"), (&comparison.removed, "removed")] {
        for name in names {
            let name = crate::util::sanitize::escape_control_chars(name).into_owned();
            rows.push([
                name,
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                label.to_string(),
            ]);
        }
    }
    rows
}

const HEADER: [&str; 6] = [
    "Command",
    "Baseline",
    "Current",
    "Change",
    "Significance",
    "",
];

/// The comparison table for the terminal.
pub fn terminal_table(
    comparison: &Comparison<'_>,
    baseline_path: &str,
    threshold: Option<f64>,
    unit: Option<Unit>,
) -> String {
    let rows = rows(comparison, threshold, unit);
    let mut widths = HEADER.map(|h| h.chars().count());
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let line = |cells: &[String; 6]| {
        let mut out = String::new();
        for (i, (cell, width)) in cells.iter().zip(widths).enumerate() {
            let pad = width - cell.chars().count();
            // Right-align the numeric columns
            if (1..=3).contains(&i) {
                out.push_str(&format!("  {}{cell}", " ".repeat(pad)));
            } else {
                out.push_str(&format!("  {cell}{}", " ".repeat(pad)));
            }
        }
        out.trim_end().to_string()
    };
    let mut out = format!(
        "Comparison with {}\n",
        crate::util::sanitize::escape_control_chars(baseline_path)
    );
    out.push_str(&line(&HEADER.map(String::from)));
    out.push('\n');
    for row in &rows {
        out.push_str(&line(row));
        out.push('\n');
    }
    out
}

/// The comparison as a Markdown table (for PR comments / CI summaries).
pub fn markdown_table(
    comparison: &Comparison<'_>,
    threshold: Option<f64>,
    unit: Option<Unit>,
) -> String {
    let escape = |cell: &str| cell.replace('|', "\\|");
    let mut out = String::from("| Command | Baseline | Current | Change | Significance | |\n");
    out.push_str("|:---|---:|---:|---:|:---|:---|\n");
    for row in rows(comparison, threshold, unit) {
        let [name, rest @ ..] = row;
        out.push_str(&format!("| `{}` |", escape(&name)));
        for cell in rest {
            let cell = if cell == "REGRESSION" {
                "**REGRESSION**".to_string()
            } else {
                escape(&cell)
            };
            out.push_str(&format!(" {cell} |"));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(command: &str, times: &[f64]) -> BenchmarkResult {
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        BenchmarkResult {
            command: command.to_string(),
            mean,
            stddev: Some(0.0),
            times: Some(times.to_vec()),
            ..Default::default()
        }
    }

    #[test]
    fn matches_by_command_and_lists_new_and_removed() {
        let baseline = [result("a", &[1.0; 5]), result("gone", &[1.0; 5])];
        let current = [result("a", &[1.2; 5]), result("added", &[1.0; 5])];
        let c = compare(&baseline, &current);
        assert_eq!(c.deltas.len(), 1);
        assert_eq!(c.deltas[0].name, "a");
        assert!((c.deltas[0].rel_change - 0.2).abs() < 1e-12);
        assert_eq!(c.new, ["added"]);
        assert_eq!(c.removed, ["gone"]);
    }

    #[test]
    fn regression_needs_threshold_and_significance() {
        let noisy_base = [1.0, 1.4, 0.8, 1.2, 0.6, 1.0];
        let noisy_cur = [1.1, 1.5, 0.9, 1.3, 0.7, 1.1];
        let baseline = [
            result("noise", &noisy_base),
            result("slow", &[1.0, 1.01, 0.99, 1.0, 1.0]),
        ];
        let current = [
            result("noise", &noisy_cur),
            result("slow", &[1.5, 1.51, 1.49, 1.5, 1.5]),
        ];
        let c = compare(&baseline, &current);
        let noise = &c.deltas[0];
        let slow = &c.deltas[1];

        // +10%, but indistinguishable from noise
        assert!(
            noise.rel_change > 0.05 && !noise.is_significant(),
            "{noise:?}"
        );
        assert!(!noise.is_regression(0.05));

        assert!(slow.is_significant(), "{slow:?}");
        assert!(slow.is_regression(0.05));
        assert!(!slow.is_regression(0.6)); // +50% is below a 60% threshold
        assert_eq!(c.regressions(0.05).count(), 1);
    }

    #[test]
    fn improvements_are_never_regressions() {
        let c_base = [result("a", &[2.0, 2.01, 1.99, 2.0, 2.0])];
        let c_cur = [result("a", &[1.0, 1.01, 0.99, 1.0, 1.0])];
        let c = compare(&c_base, &c_cur);
        assert!(c.deltas[0].is_significant());
        assert!(!c.deltas[0].is_regression(0.0));
        assert_eq!(verdict(&c.deltas[0], Some(0.05)), "faster");
    }

    #[test]
    fn constant_samples_and_missing_times() {
        let base = [result("a", &[1.0; 5]), result("b", &[1.0; 5])];
        let mut cur = [result("a", &[2.0; 5]), result("b", &[1.0; 5])];
        let c = compare(&base, &cur);
        assert_eq!(c.deltas[0].p_value, Some(0.0)); // certainly different
        assert_eq!(c.deltas[1].p_value, Some(1.0)); // identical

        cur[0].times = None;
        let c = compare(&base, &cur);
        assert_eq!(c.deltas[0].p_value, None);
        assert!(!c.deltas[0].is_regression(0.0));
    }

    #[test]
    fn thresholds() {
        assert_eq!(parse_threshold("5%"), Ok(0.05));
        assert_eq!(parse_threshold("5"), Ok(0.05));
        assert_eq!(parse_threshold(" 0.5 % "), Ok(0.005));
        assert!(parse_threshold("-1%").is_err());
        assert!(parse_threshold("fast").is_err());
    }

    #[test]
    fn tables() {
        let base = [result("a|b", &[1.0; 5]), result("gone", &[1.0; 5])];
        let cur = [result("a|b", &[1.5; 5])];
        let c = compare(&base, &cur);

        let md = markdown_table(&c, Some(0.05), None);
        assert_eq!(
            md,
            "| Command | Baseline | Current | Change | Significance | |\n\
             |:---|---:|---:|---:|:---|:---|\n\
             | `a\\|b` | 1.000 s ± 0.000 | 1.500 s ± 0.000 | +50.0% ▲ | p < 0.001 | **REGRESSION** |\n\
             | `gone` |  |  |  |  | removed |\n"
        );

        let terminal = terminal_table(&c, "base.json", None, None);
        assert!(terminal.starts_with("Comparison with base.json\n"));
        let row = terminal.lines().find(|l| l.contains("a|b")).unwrap();
        assert!(row.contains("+50.0% ▲  p < 0.001"), "{terminal}");
        assert!(row.ends_with("slower"), "{terminal}");
        assert!(terminal.contains("gone"));
    }
}
