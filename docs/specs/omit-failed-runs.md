# Specification: Omit Failed Runs from Summary Statistics (`--omit-failed-runs`)

**Issue:** [#76](https://github.com/UnitedOpen-Source/joulex/issues/76)  
**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** Completed  
**Upstream Reference:** `sharkdp/hyperfine#891` (fixes `sharkdp/hyperfine#827`)

---

## 1. Overview & Context

When benchmarking commands that are flaky or occasionally fail (e.g. distributed tests, network-dependent calls, or stress workloads), users currently face an all-or-nothing dilemma:
1. Without `--ignore-failure`, a single non-zero exit code terminates the entire benchmark immediately, wasting previous runs.
2. With `--ignore-failure`, failed runs (which often fail almost instantly or trigger timeouts) are bundled into the summary statistics, severely corrupting the mean, standard deviation, and percentiles of legitimate successful runs.

`--omit-failed-runs` bridges this gap:
- When used with `--ignore-failure`, runs with non-zero exit codes are excluded from statistical calculations (`mean`, `median`, `stddev`, `min`, `max`, `times`, `user_times`, `system_times`, `energy_measurements`, `memory_usage_byte`).
- The terminal output highlights how many runs succeeded and how many were omitted:
  `Range (min … max): 10.1 ms … 12.3 ms 8 runs (2 failed runs omitted)`
- A warning is emitted detailing: `Omitted 2 of 10 benchmark runs with non-zero exit codes from the summary statistics.`
- If all runs fail, an informative error is returned:
  `All benchmark runs failed. No successful runs to compute statistics from.`

---

## 2. Technical Architecture & Contracts

### 2.1 CLI Interface (`src/cli.rs`)
- Add `--omit-failed-runs`:
  - `ArgAction::SetTrue`
  - `requires("ignore-failure")`
  - Long help explaining usage and requirement of `--ignore-failure`.

### 2.2 Options & Error Handling (`src/options.rs`, `src/error.rs`)
- Add `pub omit_failed_runs: bool` to `Options`.
- If `options.omit_failed_runs` is set while `options.command_failure_action == CmdFailureAction::RaiseError`:
  Return `Err(OptionsError::OmitFailedRunsRequiresIgnoreFailure.into())`.
- Add `OptionsError::OmitFailedRunsRequiresIgnoreFailure` variant with user-friendly message:
  `"The '--omit-failed-runs' option requires '--ignore-failure' to be set, otherwise joulex aborts on the first failed run"`.

### 2.3 Warning Variant (`src/output/warnings.rs`)
- Add `Warnings::FailedRunsOmitted { omitted: usize, total: usize }`.
- Format:
  `"Omitted {omitted} of {total} benchmark runs with non-zero exit codes from the summary statistics."`

### 2.4 Benchmark Runner (`src/benchmark/mod.rs`)
- In `BenchmarkRunner::finish`:
  - If `self.options.omit_failed_runs`:
    - Identify `keep_indices` where `exit_code == &Some(0)`.
    - `num_omitted = original_count.saturating_sub(keep_indices.len())`.
    - If `keep_indices.is_empty()`: `bail!("All benchmark runs failed. No successful runs to compute statistics from.");`.
    - If `num_omitted > 0`:
      - Filter `times_real`, `times_user`, `times_system`, `memory_usage_byte`, and `energy_measurements`.
      - Record warning `Warnings::FailedRunsOmitted { omitted: num_omitted, total: original_count }`.
      - Adjust terminal runs line: `{t_num} runs ({num_omitted} failed runs omitted)`.
      - Maintain `exit_codes` tracking in `BenchmarkResult` (or keep full exit codes so callers can inspect failure codes).

---

## 3. Test Plan

1. **CLI Validation Tests (`tests/integration_tests.rs`):**
   - `--omit-failed-runs` without `--ignore-failure` fails with informative error.
2. **End-to-End Functionality (`tests/integration_tests.rs`):**
   - Mixed run where 2 of 5 runs exit non-zero:
     - Command succeeds.
     - Terminal displays `3 runs (2 failed runs omitted)`.
     - Stderr contains `Omitted 2 of 5 benchmark runs with non-zero exit codes`.
     - Exported JSON contains 3 times entries in `times`.
3. **All Runs Failed (`tests/integration_tests.rs`):**
   - Command fails on all runs with `--ignore-failure --omit-failed-runs`.
   - Returns error: `All benchmark runs failed`.
