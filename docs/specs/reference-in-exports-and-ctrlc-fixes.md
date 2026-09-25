# Spec: Reference Baseline in Exports and Ctrl-C Correctness Fixes

## 1. Problem Statement
Issue #113 code review identified several remaining defects and inconsistencies:
1. **Exports Ignore `--reference` (#109):** All export formats (`--export-markdown`, `--export-json`, `--export-csv`, `--export-html`, `--export-asciidoc`, `--export-orgmode`) calculate relative speeds by calling `relative_speed::compute_with_check(results, ...)`, which unconditionally chooses the fastest command as the baseline `1.00`. When `--reference <CMD>` is specified, the terminal output uses the user-selected reference, while all exports continue to report the fastest result as `1.00`.
2. **Truncated Run Recorded on Graceful SIGINT Exit (Ctrl-C Review Finding 1):** If a child command traps `SIGINT` and exits `0` (e.g. servers, Python scripts with signal handlers, graceful CLI shutdowns), `executor.rs` checks `!result.status.success()`, so the truncated run was recorded as a valid measurement.
3. **Unbenchmarked Commands Count with `--import-json` (Ctrl-C Review Finding 2):** When `--import-json` is combined with live benchmarks and an interruption occurs, `self.results.len()` includes imported results, under-counting the number of unbenchmarked commands.
4. **Flaky Unit Test in `interrupt.rs` (Ctrl-C Review Finding 4):** `test_interrupt_flag` mutated the process-global atomic `PRESSES`, which could cause concurrently running tests to treat themselves as interrupted.
5. **Real Errors Swallowed in `main.rs` (Ctrl-C Review Finding 5):** In `main.rs`, any error returning while `interrupted()` is true exited `130` without printing the error. Only `Interrupted` errors should be silent; actual operational failures (e.g. I/O failure during export) must be reported before exiting.
6. **Leftover Rebrand File:** `doc/hyperfine.1` was still present in the repository alongside `doc/joulex.1`.

## 2. Design and Implementation Plan

### 2.1 Pass Reference to Exporters (`src/export/`)
- Update `Exporter::serialize`:
  ```rust
  fn serialize(
      &self,
      results: &[BenchmarkResult],
      reference: Option<&BenchmarkResult>,
      unit: Option<Unit>,
      sort_order: SortOrder,
  ) -> Result<Vec<u8>>;
  ```
- In all exporters (`csv`, `json`, `html`, `markup`):
  Determine baseline as:
  ```rust
  let baseline = reference.unwrap_or_else(|| relative_speed::fastest_of(results));
  ```
  And compute relative speeds using `relative_speed::compute_with_check_from_reference(results, baseline, ...)`.
- Update `ExportManager::write_results` and `Scheduler::final_export` to pass `reference: Option<&BenchmarkResult>`.

### 2.2 Fix Truncated Run Detection in Executor (`src/benchmark/executor.rs`)
- Record `let interrupted_before = crate::util::interrupt::interrupted();` before running the command.
- If `!interrupted_before && crate::util::interrupt::interrupted()`, bail `crate::error::Interrupted` regardless of the child process exit status.

### 2.3 Correct Unbenchmarked Count in Scheduler (`src/benchmark/scheduler.rs`)
- Track `imported_count: usize` in `Scheduler`.
- Calculate completed live benchmarks as `self.results.len().saturating_sub(self.imported_count)`.
- Compute `unbenchmarked = total_live_commands.saturating_sub(completed_live_commands)`.

### 2.4 Error Handling in `src/main.rs`
- In `main()` error handler:
  ```rust
  if e.is::<crate::error::Interrupted>() {
      std::process::exit(130);
  }
  eprintln!("{} {:#}", colors::red("Error:"), e);
  if crate::util::interrupt::interrupted() {
      std::process::exit(130);
  }
  std::process::exit(1);
  ```

### 2.5 Eliminate Flaky Unit Test in `src/util/interrupt.rs`
- Remove `test_interrupt_flag` and `trigger()`. Interruption behavior is tested via separate child processes in `tests/integration_tests.rs`.

## 3. Verification & Testing
- Unit and integration tests for `--reference` in `--export-markdown`, `--export-json`, `--export-csv`, and `--export-html`.
- Run full test suite (`cargo test`), format check (`cargo fmt --check`), and linter (`cargo clippy --all-targets -- -D warnings`).
