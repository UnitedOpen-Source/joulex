//! Self-contained HTML report (`--export-html`): summary table, histogram with
//! kernel density estimate, run-order plot per command, and a box plot that
//! compares all commands. Pure inline SVG and CSS; no JavaScript and no
//! external assets, so the file works offline and as a CI artifact.

use std::fmt::Write;

use super::markup::determine_unit_from_results;
use super::Exporter;
use crate::benchmark::benchmark_result::BenchmarkResult;
use crate::benchmark::relative_speed;
use crate::options::SortOrder;
use crate::output::format::format_duration_value;
use crate::stats::summary::percentile;
use crate::util::units::Unit;

use anyhow::Result;

#[derive(Default)]
pub struct HtmlExporter {}

const WIDTH: f64 = 640.0;

const STYLE: &str = r#"
:root { --fg: #1f2328; --muted: #59636e; --bg: #ffffff; --grid: #d1d9e0; --bar: #6ea8fe; --line: #0b5ed7; --dot: #0b5ed7; }
@media (prefers-color-scheme: dark) {
  :root { --fg: #e6edf3; --muted: #9198a1; --bg: #0d1117; --grid: #3d444d; --bar: #1f6feb; --line: #79c0ff; --dot: #79c0ff; }
}
body { font-family: system-ui, -apple-system, sans-serif; color: var(--fg); background: var(--bg); max-width: 720px; margin: 2rem auto; padding: 0 16px; }
h1 { font-size: 1.5rem; } h2 { font-size: 1.1rem; margin-top: 2rem; }
code { font-family: ui-monospace, monospace; }
table { border-collapse: collapse; width: 100%; font-size: 0.9rem; }
th, td { border-bottom: 1px solid var(--grid); padding: 4px 8px; text-align: right; }
th:first-child, td:first-child { text-align: left; }
svg { width: 100%; height: auto; display: block; }
svg text { fill: var(--muted); font-size: 11px; font-family: system-ui, sans-serif; }
.axis { stroke: var(--grid); stroke-width: 1; }
.bar { fill: var(--bar); opacity: 0.6; }
.kde { fill: none; stroke: var(--line); stroke-width: 2; }
.dot { fill: var(--dot); }
.run { fill: none; stroke: var(--line); stroke-width: 1; opacity: 0.4; }
.box { fill: var(--bar); opacity: 0.6; stroke: var(--line); }
.whisker, .median { stroke: var(--line); stroke-width: 1.5; }
.muted { color: var(--muted); font-size: 0.85rem; }
"#;

/// Escape text for HTML element content and attribute values.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Gaussian kernel density estimate at the `grid` points, with Silverman's
/// rule-of-thumb bandwidth.
fn kde(xs: &[f64], grid: &[f64]) -> Vec<f64> {
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    let sd = (xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    let h = (1.06 * sd * n.powf(-0.2)).max(f64::EPSILON);
    let norm = 1.0 / (n * h * (2.0 * std::f64::consts::PI).sqrt());
    grid.iter()
        .map(|g| {
            norm * xs
                .iter()
                .map(|x| (-0.5 * ((g - x) / h).powi(2)).exp())
                .sum::<f64>()
        })
        .collect()
}

fn fmt_time(value: f64, unit: Unit) -> String {
    format!(
        "{} {}",
        format_duration_value(value, Some(unit)).0,
        unit.short_name()
    )
}

/// Range of the values, widened a little so that points aren't drawn on the
/// border, and never empty.
fn padded_range(values: impl Iterator<Item = f64>) -> (f64, f64) {
    let (lo, hi) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(v), hi.max(v))
    });
    let span = (hi - lo).max(hi.abs() * 1e-3).max(1e-12);
    (lo - 0.05 * span, hi + 0.05 * span)
}

fn histogram_kde_svg(times: &[f64], unit: Unit) -> String {
    let height = 200.0;
    let plot = height - 24.0;
    let (lo, hi) = padded_range(times.iter().copied());
    let span = hi - lo;
    let x = |v: f64| (v - lo) / span * WIDTH;

    let bins = ((times.len() as f64).sqrt().ceil() as usize).clamp(5, 40);
    let mut counts = vec![0usize; bins];
    for &t in times {
        let bin = (((t - lo) / span) * bins as f64) as usize;
        counts[bin.min(bins - 1)] += 1;
    }
    let max_count = *counts.iter().max().unwrap_or(&1) as f64;
    let bin_width = WIDTH / bins as f64;

    let mut svg = format!(
        r#"<svg viewBox="0 0 {WIDTH} {height}" role="img" aria-label="Histogram of run times">"#
    );
    for (i, &count) in counts.iter().enumerate() {
        let h = count as f64 / max_count * plot;
        let _ = write!(
            svg,
            r#"<rect class="bar" x="{:.1}" y="{:.1}" width="{:.1}" height="{h:.1}"/>"#,
            i as f64 * bin_width + 1.0,
            plot - h,
            (bin_width - 2.0).max(0.5)
        );
    }
    if times.len() >= 2 {
        let grid: Vec<f64> = (0..=120)
            .map(|i| lo + span * f64::from(i) / 120.0)
            .collect();
        let density = kde(times, &grid);
        let max_density = density.iter().copied().fold(f64::MIN_POSITIVE, f64::max);
        let points: Vec<String> = grid
            .iter()
            .zip(&density)
            .map(|(g, d)| format!("{:.1},{:.1}", x(*g), plot - d / max_density * plot))
            .collect();
        let _ = write!(
            svg,
            r#"<polyline class="kde" points="{}"/>"#,
            points.join(" ")
        );
    }
    let _ = write!(
        svg,
        r#"<line class="axis" x1="0" y1="{plot}" x2="{WIDTH}" y2="{plot}"/><text x="0" y="{}">{}</text><text x="{WIDTH}" y="{}" text-anchor="end">{}</text></svg>"#,
        height - 6.0,
        escape_html(&fmt_time(lo, unit)),
        height - 6.0,
        escape_html(&fmt_time(hi, unit))
    );
    svg
}

fn run_order_svg(times: &[f64], unit: Unit) -> String {
    let height = 140.0;
    let plot = height - 20.0;
    let (lo, hi) = padded_range(times.iter().copied());
    let n = times.len().max(2) as f64 - 1.0;
    let point = |i: usize, t: f64| {
        (
            i as f64 / n * (WIDTH - 20.0) + 10.0,
            plot - (t - lo) / (hi - lo) * (plot - 10.0),
        )
    };

    let mut svg = format!(
        r#"<svg viewBox="0 0 {WIDTH} {height}" role="img" aria-label="Run times in run order">"#
    );
    let path: Vec<String> = times
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            let (px, py) = point(i, t);
            format!("{px:.1},{py:.1}")
        })
        .collect();
    let _ = write!(
        svg,
        r#"<polyline class="run" points="{}"/>"#,
        path.join(" ")
    );
    for (i, &t) in times.iter().enumerate() {
        let (px, py) = point(i, t);
        let _ = write!(
            svg,
            r#"<circle class="dot" cx="{px:.1}" cy="{py:.1}" r="2.5"/>"#
        );
    }
    let _ = write!(
        svg,
        r#"<line class="axis" x1="0" y1="{plot}" x2="{WIDTH}" y2="{plot}"/><text x="0" y="{}">run 1</text><text x="{WIDTH}" y="{}" text-anchor="end">run {} · range {} – {}</text></svg>"#,
        height - 4.0,
        height - 4.0,
        times.len(),
        escape_html(&fmt_time(lo, unit)),
        escape_html(&fmt_time(hi, unit))
    );
    svg
}

fn boxplot_svg(results: &[&BenchmarkResult], unit: Unit) -> String {
    let row = 34.0;
    let label_width = 0.0;
    let height = row * results.len() as f64 + 24.0;
    let (lo, hi) = padded_range(
        results
            .iter()
            .flat_map(|r| r.times.iter().flatten().copied()),
    );
    let x = |v: f64| label_width + (v - lo) / (hi - lo) * (WIDTH - label_width);

    let mut svg = format!(
        r#"<svg viewBox="0 0 {WIDTH} {height}" role="img" aria-label="Box plot of all commands">"#
    );
    for (i, result) in results.iter().enumerate() {
        let mut times = result.times.clone().unwrap_or_default();
        times.sort_by(f64::total_cmp);
        let q = |p: f64| percentile(&times, p).unwrap_or(f64::NAN);
        let (min, q1, med, q3, max) = (q(0.0), q(25.0), q(50.0), q(75.0), q(100.0));
        let y = row * i as f64 + 6.0;
        let mid = y + 10.0;
        let _ = write!(
            svg,
            r#"<line class="whisker" x1="{:.1}" y1="{mid:.1}" x2="{:.1}" y2="{mid:.1}"/><rect class="box" x="{:.1}" y="{y:.1}" width="{:.1}" height="20"/><line class="median" x1="{m:.1}" y1="{y:.1}" x2="{m:.1}" y2="{:.1}"/><text x="{:.1}" y="{:.1}">{}</text>"#,
            x(min),
            x(max),
            x(q1),
            (x(q3) - x(q1)).max(1.0),
            y + 20.0,
            x(min),
            y + 31.0,
            escape_html(&result.command_with_unused_parameters),
            m = x(med),
        );
    }
    let _ = write!(
        svg,
        r#"<text x="0" y="{}">{}</text><text x="{WIDTH}" y="{}" text-anchor="end">{}</text></svg>"#,
        height - 2.0,
        escape_html(&fmt_time(lo, unit)),
        height - 2.0,
        escape_html(&fmt_time(hi, unit))
    );
    svg
}

fn summary_table(results: &[BenchmarkResult], unit: Unit, sort_order: SortOrder) -> String {
    let entries = relative_speed::compute_with_check(results, sort_order).unwrap_or_else(|| {
        relative_speed::compute_without_ratios(
            results,
            relative_speed::fastest_of(results),
            sort_order,
        )
    });
    let mut table = format!(
        "<table><thead><tr><th>Command</th><th>Mean [{0}]</th><th>Min [{0}]</th><th>Max [{0}]</th><th>Relative</th></tr></thead><tbody>",
        unit.short_name()
    );
    for entry in &entries {
        let r = entry.result;
        let value = |v: f64| format_duration_value(v, Some(unit)).0;
        let stddev = r
            .stddev
            .map(|s| format!(" ± {}", value(s)))
            .unwrap_or_default();
        let relative = if entry.relative_speed.is_finite() {
            format!("{:.2}", entry.relative_speed)
        } else {
            "n/a".into()
        };
        let _ = write!(
            table,
            "<tr><td><code>{}</code></td><td>{}{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&r.command_with_unused_parameters),
            value(r.mean),
            stddev,
            value(r.min),
            value(r.max),
            relative
        );
    }
    table.push_str("</tbody></table>");
    table
}

impl Exporter for HtmlExporter {
    fn serialize(
        &self,
        results: &[BenchmarkResult],
        unit: Option<Unit>,
        sort_order: SortOrder,
    ) -> Result<Vec<u8>> {
        let unit = unit.unwrap_or_else(|| determine_unit_from_results(results));
        let mut html = format!(
            "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
             <title>joulex report</title><style>{STYLE}</style></head><body>\
             <h1>joulex benchmark report</h1><p class=\"muted\">joulex {}</p>",
            env!("CARGO_PKG_VERSION")
        );

        if results.is_empty() {
            html.push_str("<p>No results.</p></body></html>\n");
            return Ok(html.into_bytes());
        }

        html.push_str("<h2>Summary</h2>");
        html.push_str(&summary_table(results, unit, sort_order));

        let with_times: Vec<&BenchmarkResult> = results
            .iter()
            .filter(|r| r.times.as_ref().is_some_and(|t| !t.is_empty()))
            .collect();
        if with_times.len() > 1 {
            html.push_str("<h2>Comparison</h2>");
            html.push_str(&boxplot_svg(&with_times, unit));
        }

        for result in &with_times {
            let times = result.times.as_deref().unwrap_or_default();
            let _ = write!(
                html,
                "<h2><code>{}</code></h2><p class=\"muted\">{} runs · histogram with kernel density estimate, then run times in execution order</p>",
                escape_html(&result.command_with_unused_parameters),
                times.len()
            );
            html.push_str(&histogram_kde_svg(times, unit));
            html.push_str(&run_order_svg(times, unit));
        }

        html.push_str("</body></html>\n");
        Ok(html.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(command: &str, times: Vec<f64>) -> BenchmarkResult {
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        BenchmarkResult {
            command: command.into(),
            command_with_unused_parameters: command.into(),
            mean,
            stddev: Some(0.001),
            median: mean,
            min: times.iter().copied().fold(f64::INFINITY, f64::min),
            max: times.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            times: Some(times),
            ..Default::default()
        }
    }

    fn render(results: &[BenchmarkResult]) -> String {
        String::from_utf8(
            HtmlExporter::default()
                .serialize(results, Some(Unit::MilliSecond), SortOrder::Command)
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn escapes_all_html_special_characters() {
        assert_eq!(
            escape_html(r#"<a href="x">'&'</a>"#),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn kde_integrates_to_about_one() {
        let xs = [1.0, 1.1, 0.9, 1.05, 0.95, 1.2];
        let grid: Vec<f64> = (0..=2000).map(|i| f64::from(i) * 0.001).collect();
        let density = kde(&xs, &grid);
        let integral: f64 = density.iter().sum::<f64>() * 0.001;
        assert!((integral - 1.0).abs() < 0.01, "{integral}");
    }

    #[test]
    fn report_is_self_contained_and_has_all_sections() {
        let html = render(&[
            result("sleep 0.1", vec![0.100, 0.101, 0.099, 0.102]),
            result("sleep 0.2", vec![0.200, 0.201, 0.199, 0.203]),
        ]);
        assert!(html.starts_with("<!doctype html>"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("http://") && !html.contains("https://"));
        assert!(html.contains("<table>") && html.contains("Relative"));
        assert!(html.contains("aria-label=\"Box plot of all commands\""));
        assert_eq!(
            html.matches("aria-label=\"Histogram of run times\"")
                .count(),
            2
        );
        assert_eq!(
            html.matches("aria-label=\"Run times in run order\"")
                .count(),
            2
        );
        assert!(html.contains("prefers-color-scheme: dark"));
    }

    #[test]
    fn command_names_cannot_inject_markup() {
        let html = render(&[result("<script>alert(1)</script>", vec![0.1, 0.1, 0.1])]);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    }

    #[test]
    fn constant_and_single_run_samples_render_without_nan() {
        let html = render(&[result("a", vec![0.1, 0.1, 0.1]), result("b", vec![0.2])]);
        assert!(!html.contains("NaN") && !html.contains("inf"), "{html}");
    }

    #[test]
    fn empty_results_give_a_valid_page() {
        let html = render(&[]);
        assert!(html.contains("No results.") && html.ends_with("</html>\n"));
    }
}
