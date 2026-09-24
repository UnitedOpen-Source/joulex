# Specification: Correct Significance in `--deep-stats` for Zero Variance & Identical Samples

**Issue:** [#101](https://github.com/UnitedOpen-Source/joulex/issues/101)  
**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** Completed  
**Upstream Reference:** N/A (joulex feature)

---

## 1. Overview & Context

When using `--deep-stats` with `--reference`, `joulex` performs two-sample Welch's t-test bootstrapping between the reference benchmark times and each comparison command to compute a $p$-value and determine if differences are statistically significant ($p < 0.05$ or $p < 0.01$).

However, when both samples have zero variance (such as identical constant vectors, e.g. `[1.0, 1.0, 1.0, 1.0]`, coarse system timers on Windows, mock commands, or clamped `--subtract` timings):
1. `Sample::t(&self, &other)` divides by pooled standard error `0.0 / 0.0 = NaN`.
2. When `t_stat` is `NaN`, `distribution.p_value(NaN, &Tails::Two)` yields `0.0`.
3. Because `0.0 < 0.01`, `is_significant_01` became `true`.
4. As a result, the tool printed:
   `[Bootstrap t-test: t = NaN, p = 0.0000 -> statistically significant (p < 0.01)]`
   reporting identical samples as statistically significantly different!

This change ensures:
1. When two constant samples with zero variance have equal means, they are recognized as identical (`p_value: 1.0`, `t_statistic: 0.0`, `is_significant: false`).
2. When two samples have zero variance but different means, or when $p$-value evaluation is mathematically undefined, `compare_samples` returns `None`.
3. When `None` is returned, `scheduler.rs` gracefully reports `[Bootstrap t-test: not applicable (zero variance or insufficient samples)]` rather than printing incorrect or misleading results.

---

## 2. Technical Architecture & Contracts

### 2.1 Sample Comparison (`src/stats/deep.rs`)
- In `compare_samples(a: &[f64], b: &[f64]) -> Option<ComparisonStats>`:
  - If `a.len() < 3 || b.len() < 3`, return `None`.
  - Check `t_stat = sample_a.t(sample_b)`.
  - If `!t_stat.is_finite()`:
    - Compare sample means with floating point tolerance:
      `let same = (sample_a.mean() - sample_b.mean()).abs() <= f64::EPSILON * sample_a.mean().abs().max(1.0);`
    - If `same`, return:
      ```rust
      Some(ComparisonStats {
          p_value: 1.0,
          t_statistic: 0.0,
          is_significant_05: false,
          is_significant_01: false,
      })
      ```
    - Otherwise (different means with zero variance), return `None`.
  - For normal finite `t_stat`, compute bootstrap distribution and `p_val = dist.p_value(t_stat, &Tails::Two)`.
  - If `!p_val.is_finite()`, return `None`.

### 2.2 Terminal Output (`src/benchmark/scheduler.rs`)
- If `compare_samples` returns `Some(cmp)`:
  - Format output as before: `[Bootstrap t-test: t = {:.2}, p = {:.4} -> {}]`.
- If `compare_samples` returns `None`:
  - Print informative status: `[Bootstrap t-test: not applicable (zero variance or insufficient samples)]`.

---

## 3. Test Plan

### 3.1 Unit Tests (`src/stats/deep.rs`)
- `test_compare_samples_identical_constant`:
  - Input: identical vectors `vec![1.0, 1.0, 1.0, 1.0]`.
  - Asserts `cmp.p_value == 1.0`, `!cmp.is_significant_05`, `!cmp.is_significant_01`, `cmp.t_statistic == 0.0`.
- `test_compare_samples_different_constant`:
  - Input: `vec![1.0, 1.0, 1.0]` and `vec![2.0, 2.0, 2.0]`.
  - Asserts `compare_samples` returns `None`.
- `test_compare_samples_insufficient_samples`:
  - Input: vectors with length < 3 return `None`.
- Existing `test_compare_samples_significance` passes.

### 3.2 Integration Test (`tests/integration_tests.rs`)
- Test `--import-json` with identical samples and `--deep-stats`:
  - Output contains `p = 1.0000 -> no statistically significant difference`.
  - Does NOT contain `t = NaN` or `significant (p < 0.01)`.
