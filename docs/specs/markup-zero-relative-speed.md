# Specification: Prevent `inf`/`NaN` in Markup Exports When Benchmark Mean Is Zero

**Issue:** [#95](https://github.com/UnitedOpen-Source/joulex/issues/95)  
**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** Completed  
**Upstream Reference:** `sharkdp/hyperfine#319`, `sharkdp/hyperfine#642`

---

## 1. Overview & Context

When exporting benchmark results to table-based markup formats (Markdown, AsciiDoc, Emacs Org-mode), `joulex` computes relative speeds across commands using the fastest run (or reference command) as the baseline.

However, when a benchmark has a mean of 0.0 (which occurs with fast mock commands, low-resolution timer environments, or `--subtract` results clamped to zero), dividing by zero causes relative speed calculations to yield `f64::INFINITY` or `NaN`.
While the terminal output detects zero-time benchmarks and gracefully prints a warning without computing relative comparisons, `MarkupExporter` previously called `relative_speed::compute(results, sort_order)` directly, printing:
```markdown
| `a` | 0.0 ± 0.0 | 0.0 | 0.0 | 1.00 |
| `b` | 0.0 ± 0.0 | 0.0 | 0.0 | inf |
```
or `± NaN` in the table output.

This fix ensures:
1. When zero-time benchmarks prevent meaningful ratio calculation, relative speeds and uncertainties safely degrade to `"n/a"`.
2. Markup exporters never emit `inf` or `NaN` into output documents.
3. Empty result sets or edge-case results are handled without panics.

---

## 2. Technical Architecture & Contracts

### 2.1 Relative Speed Calculation (`src/benchmark/relative_speed.rs`)
- In `compute_with_check`:
  - Handle empty `results` gracefully: return `Some(Vec::new())`.
  - If `fastest.mean == 0.0`, return `None`.
- In `compute_with_check_from_reference`:
  - Handle empty `results` gracefully: return `Some(Vec::new())`.
  - If `fastest.mean == 0.0` or `reference.mean == 0.0`, return `None`.
- Add `compute_without_ratios<'a>(results: &'a [BenchmarkResult], sort_order: SortOrder) -> Vec<BenchmarkResultWithRelativeSpeed<'a>>`:
  - Used as fallback when relative ratios cannot be computed.
  - Sets `relative_speed = f64::NAN` and `relative_speed_stddev = None` for each entry while preserving ordering and result metadata.
- In `compute`:
  - Handle empty `results` gracefully: return `Vec::new()`.

### 2.2 Markup Exporter Serialization (`src/export/markup.rs`)
- In `MarkupExporter::serialize`:
  ```rust
  let entries = relative_speed::compute_with_check(results, sort_order)
      .unwrap_or_else(|| relative_speed::compute_without_ratios(results, sort_order));
  ```
- In `MarkupExporter::table_results`:
  - Guard `entry.relative_speed`:
    ```rust
    let rel_str = if entry.relative_speed.is_finite() {
        format!("{:.2}", entry.relative_speed)
    } else {
        "n/a".into()
    };
    ```
  - Guard `entry.relative_speed_stddev`:
    ```rust
    let rel_stddev_str = if entry.is_reference {
        "".into()
    } else if let Some(stddev) = entry.relative_speed_stddev {
        if stddev.is_finite() {
            format!(" ± {stddev:.2}")
        } else {
            "".into()
        }
    } else {
        "".into()
    };
    ```

---

## 3. Test Plan

### 3.1 Unit & Snapshot Tests (`src/export/tests.rs`)
- Add test `test_markup_export_zero_mean_displays_na`:
  - Construct benchmark results with 0.0 mean.
  - Verify Markdown, AsciiDoc, and Org-mode output contain `"n/a"` for relative speed.
  - Verify neither `"inf"` nor `"NaN"` appears in the generated output.
- Verify all existing snapshot tests pass unchanged.

### 3.2 Relative Speed Tests (`src/benchmark/relative_speed.rs`)
- Verify `compute_without_ratios` properly populates results and honors sort orders.
- Verify `compute_with_check` handles empty inputs and zero-duration baselines.
