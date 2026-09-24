# Specification: Pointer-Based Reference Comparison & Explicit Reference in `compute_without_ratios`

**Issue:** [#109](https://github.com/UnitedOpen-Source/joulex/issues/109)  
**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** Completed  
**Upstream Reference:** N/A (joulex bugfix)

---

## 1. Overview & Context

In `src/benchmark/relative_speed.rs`, `is_reference` was previously evaluated using `result == reference` (value equality via `PartialEq` on `BenchmarkResult`).
When two results have identical values (such as identical zero-runtime fixtures or identical mock commands):
1. Both results were flagged with `is_reference: true`.
2. `compute_without_ratios` always assumed `fastest` was the reference, ignoring explicit `--reference` commands passed from callers.

This change ensures:
1. `is_reference` is determined strictly by identity (`std::ptr::eq(result, reference)`), guaranteeing that exactly one entry in the result vector is marked as the reference even when values are identical.
2. `compute_without_ratios` accepts an explicit `reference: &'a BenchmarkResult` parameter, matching `compute_with_check_from_reference`.
3. In `MarkupExporter::serialize`, `compute_without_ratios` is invoked with `fastest` (or the reference).

---

## 2. Technical Architecture & Contracts

### 2.1 Relative Speed Calculations (`src/benchmark/relative_speed.rs`)
- In `compute_relative_speeds`:
  - Change `let is_reference = result == reference;` to `let is_reference = std::ptr::eq(result, reference);`.
- In `compute_without_ratios`:
  - Signature:
    ```rust
    pub fn compute_without_ratios<'a>(
        results: &'a [BenchmarkResult],
        reference: &'a BenchmarkResult,
        sort_order: SortOrder,
    ) -> Vec<BenchmarkResultWithRelativeSpeed<'a>>
    ```
  - Mark `is_reference: std::ptr::eq(result, reference)`.
  - Mark `relative_ordering: compare_mean_time(result, reference)`.

### 2.2 Markup Serialization (`src/export/markup.rs`)
- In `MarkupExporter::serialize`:
  ```rust
  let unit = unit.unwrap_or_else(|| determine_unit_from_results(results));
  let entries = if results.is_empty() {
      Vec::new()
  } else {
      let fastest = relative_speed::fastest_of(results);
      relative_speed::compute_with_check(results, sort_order)
          .unwrap_or_else(|| relative_speed::compute_without_ratios(results, fastest, sort_order))
  };
  ```

---

## 3. Test Plan

### 3.1 Unit Tests (`src/benchmark/relative_speed.rs`)
- `test_compute_without_ratios_identical_results_single_reference`:
  - Pass two identical `BenchmarkResult` instances.
  - Assert that exactly ONE entry has `is_reference: true`.
- `test_compute_relative_speeds_identical_results_single_reference`:
  - Pass two identical `BenchmarkResult` instances to `compute_with_check`.
  - Assert that exactly ONE entry has `is_reference: true`.
- `test_compute_without_ratios_explicit_reference`:
  - Pass two results with explicit reference to the second.
  - Assert `entries[1].is_reference == true` and `entries[0].is_reference == false`.
