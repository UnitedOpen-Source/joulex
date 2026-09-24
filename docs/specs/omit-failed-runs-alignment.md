# Spec: Fix --omit-failed-runs Exit Codes Alignment & Interaction with --filter-failed (Issue #105)

## 1. Context & Motivation
When `--omit-failed-runs` was introduced (PR #99 / Issue #76), it filtered execution metrics (`times_real`, `times_user`, `times_system`, `memory_usage_byte`) to exclude runs that terminated with a non-zero exit code or signal.

However:
1. `self.exit_codes` was left untouched in `BenchmarkRunner::finish`. Consequently, `BenchmarkResult::has_failure()` returned `true` because `exit_codes` still retained the failed runs. When `--filter-failed` was passed alongside `--omit-failed-runs`, the entire benchmark was dropped instead of keeping the successful runs.
2. In exported JSON files (and any consumer zipping `times` and `exit_codes`), `times.len()` did not match `exit_codes.len()` (e.g. 5 vs 6).
3. If energy measurements were recorded conditionally or missing per run, the energy array wasn't consistently aligned with run indices.
4. Information about which runs failed and their exit codes was lost to consumers when omitted.

## 2. Specification & Contracts
1. **Model Representation (`BenchmarkResult` & `OmittedRun`):**
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
   pub struct OmittedRun {
       pub index: usize,
       pub exit_code: Option<i32>,
   }

   #[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
   pub struct BenchmarkResult {
       // ... existing fields ...
       #[serde(default, skip_serializing_if = "Vec::is_empty")]
       pub omitted_failed_runs: Vec<OmittedRun>,
   }
   ```
2. **Filtering Semantics in `BenchmarkRunner::finish`:**
   - Detect failed runs where `exit_code != Some(0)`.
   - Store omitted runs in `omitted_failed_runs: Vec<OmittedRun>`.
   - Filter `exit_codes` using `keep_indices` such that `exit_codes.len() == times.len()`.
   - As a result, `BenchmarkResult::has_failure()` returns `false` when all retained runs were successful (`Some(0)`), ensuring `--filter-failed` retains the benchmark.
3. **Energy Per-Run Tracking:**
   - Store `energy_measurements: Vec<Option<f64>>` in `BenchmarkRunner` so each iteration records an entry (either `Some(joules)` or `None`).
   - Filter `energy_measurements` with `keep_indices`.
   - When computing mean/watts and `energy_joules`, filter `valid_energy = energy_measurements.iter().filter_map(|&e| e).collect()`.
4. **Documentation / Help Text:**
   - In `src/cli.rs`, clarify how `--omit-failed-runs` interacts with `--filter-failed` and that omitted runs are captured in `omitted_failed_runs`.

## 3. Verification Plan
- Unit tests in `src/benchmark/benchmark_result.rs`:
  - Verify `omitted_failed_runs` serialization and deserialization.
  - Verify `has_failure()` on filtered results.
- Integration tests in `tests/integration_tests.rs`:
  - Run `--omit-failed-runs --filter-failed` with a flaky command (`$JOULEX_ITERATION = 2` failing) and another command. Assert both benchmarks are exported and retained.
  - Assert that in exported JSON:
    - `times.len() == exit_codes.len()`
    - `omitted_failed_runs` contains `[{ "index": 2, "exit_code": 1 }]`
