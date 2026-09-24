# Joulex: Upstream Improvements Specification (from Hyperfine & Bench)

**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** Approved  
**Target:** `joulex` v0.2.0  
**References:**
- `sharkdp/hyperfine` issues and PRs: #920, #921, #923, #915, #926, #882, #781, #902, #852, #881, #919, #861, #823, #523, #711.
- `Gabriella439/bench` issues and PRs: #39, #45, #14, #25, #3.

---

## 1. Executive Summary

`joulex` was established as a hard fork of `sharkdp/hyperfine` and `Gabriella439/bench` to deliver **Performance per Watt (Energy)** and **Academic Deep Statistics (Criterion-rs)**.

A systematic audit of all open issues and pull requests from both upstream repositories reveals critical safety vulnerabilities, runtime panics, environment ergonomics deficits, and missing statistical features. This specification defines the architecture, contracts, and test plan for integrating these upstream solutions directly into `joulex`.

---

## 2. Scope of Improvements

### 2.1 Critical Safety & Reliability
1. **Prevent Integer Overflow in RangeStep (`#920`, `#921`)**:
   - `RangeStep::next()` in `src/parameter/range_step.rs` can overflow near `i32::MAX`.
   - Solution: Add `finished: bool` flag and verify `self.end - self.state < self.step` before adding.
2. **Prevent Panics on Zero Runs (`#923`)**:
   - `--runs 0` or `--max-runs 0` causes subtraction overflow or division by zero.
   - Solution: Enforce validation in `Options::from_cli_arguments` rejecting `--runs 0` with `OptionsError::ZeroRuns` and clamp minimum execution count to at least 1 in `Benchmark::run`.
3. **Prevent CSV Spreadsheet Formula Injection (CWE-1236) (`#915`, `#926`)**:
   - CSV export currently outputs raw command strings and parameter values. If a string starts with `=`, `+`, `-`, `@`, `\t`, or `\r`, opening the CSV in Excel or LibreOffice can execute external commands or formulas.
   - Solution: Implement `sanitize_csv_value` prepending `'` to sensitive leading characters.
4. **Fix CSV Column Alignment for Parameterized Runs (`#852`, `#902`)**:
   - When a `--reference` command has no parameters but benchmarked commands do, the CSV output column count was mismatched.
   - Solution: Insert empty cells for unparameterized commands to maintain tabular integrity.

### 2.2 Environment Ergonomics & Tooling
5. **Pass Iteration Variables to Preparation & Conclusion Commands (`#781`, `#882`)**:
   - Users need to set up iteration-specific state in `--prepare` and `--conclude`.
   - Solution: Forward `BenchmarkIteration` into `run_preparation_command` and `run_conclusion_command`, exporting both `JOULEX_ITERATION` and `HYPERFINE_ITERATION`.
6. **Add `--filter-failed` Option (`#861`)**:
   - When using `--ignore-failure`, failed benchmark runs distort relative speed comparisons.
   - Solution: Add `--filter-failed` flag in CLI to filter out results that encountered non-zero exit codes.
7. **Unified `--export <FILE>` with Automatic Extension Detection (`#691`, `#823`)**:
   - Allow `-e, --export <FILE>` which deduces export type from extension (`.json`, `.csv`, `.md`, `.adoc`, `.org`) defaulting to JSON.
8. **Built-in Shell Completions Generator (`#881`, `#919`)**:
   - Support `--generate-completions <SHELL>` (`bash`, `zsh`, `fish`, `powershell`, `elvish`) to output completion scripts to stdout without manual build extraction.

### 2.3 System Metrics & Statistical Analysis
9. **Display Peak Memory Usage (`Gabriella439/bench#45`, `sharkdp/hyperfine#711`)**:
   - `joulex` already collects `memory_usage_byte` via OS resource usage APIs.
   - Solution: Format and print peak memory usage in terminal output alongside User and System time (e.g. `[User: 1.2 ms, System: 1.4 ms, Peak Memory: 12.4 MB]`).
10. **Statistical Hypothesis Testing (Welch's t-test / p-value) (`sharkdp/hyperfine#523`, `Gabriella439/bench#39`)**:
    - Under `--deep-stats`, when comparing multiple commands, evaluate the statistical significance of speed differences using two-sample hypothesis testing from `criterion-stats` (reporting whether differences are statistically significant at $p < 0.01$ or $p < 0.05$).

---

## 3. Implementation Steps

1. **Phase 1: Robustness Fixes**
   - Implement `RangeStep` bounds check and overflow test.
   - Add zero-runs validation error and test.
   - Implement CSV formula injection sanitization and parameter alignment.
2. **Phase 2: CLI & Ergonomics**
   - Update `executor` and `Benchmark` to forward iterations to prepare/conclude commands.
   - Add `--filter-failed` flag to CLI, Options, and Scheduler.
   - Add `--export <FILE>` option with path extension matcher.
   - Add `--generate-completions <SHELL>` to CLI and main handler.
3. **Phase 3: Deep Metrics & Terminal Output**
   - Implement memory formatter and display peak memory in summary line.
   - Enrich `--deep-stats` output with p-value calculations when comparing benchmarks.
4. **Phase 4: Verification & Git Commit**
   - Execute test suite (`cargo test`).
   - Validate CLI output and end-to-end flags.
   - Create signed commit conforming to global developer rules.
