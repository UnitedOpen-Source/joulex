# Spec: Time-Unit Aware Deep Stats Formatting & Help Text Accuracy (Issue #107)

## 1. Problem Statement
1. **Time-Unit Formatting:**
   In `BenchmarkRunner::finish` (`src/benchmark/mod.rs`), the `--deep-stats` confidence intervals were formatted with hardcoded `{:.4}s`:
   `Bootstrap 95% CI:   [mean: {:.4}s … {:.4}s, median: {:.4}s … {:.4}s]`
   This ignores `--time-unit` and the auto-scaled unit (e.g., millisecond, microsecond). For fast commands (e.g. 50 µs), output was rendered as `[mean: 0.0000s … 0.0001s]`, losing all precision.
2. **Missing Standard Deviation CI:**
   `DeepStats` computes `std_dev_ci_lower` and `std_dev_ci_upper`, but they were not displayed in the summary.
3. **Inaccurate Help Text:**
   `--deep-stats --help` promised "bootstrapped confidence intervals and kernel density estimation", but kernel density estimation (KDE) is not implemented in the CLI.

## 2. Technical Design
1. **Time-Unit Aware Output (`src/benchmark/mod.rs`):**
   Format all confidence intervals using `format_duration(val, Some(time_unit))` with the resolved benchmark time unit, matching the other lines in the terminal output:
   ```rust
   println!(
       "  Bootstrap 95% CI:   [mean: {} … {}, median: {} … {}, σ: {} … {}]",
       format_duration(deep.mean_ci_lower, Some(time_unit)),
       format_duration(deep.mean_ci_upper, Some(time_unit)),
       format_duration(deep.median_ci_lower, Some(time_unit)),
       format_duration(deep.median_ci_upper, Some(time_unit)),
       format_duration(deep.std_dev_ci_lower, Some(time_unit)),
       format_duration(deep.std_dev_ci_upper, Some(time_unit)),
   );
   ```

2. **Help Text & Man Page Correction (`src/cli.rs`, `doc/joulex.1`):**
   - In `src/cli.rs`: update `--help` for `deep-stats` to:
     `"Perform deep statistical analysis (bootstrapped confidence intervals for mean, median, and stddev, plus hypothesis testing)."`
   - In `doc/joulex.1`: document confidence intervals for mean, median, and standard deviation.

## 3. Verification Plan
- Unit & integration tests in `tests/integration_tests.rs`:
  - Run `--deep-stats` with `--time-unit=millisecond` on a command, verifying output contains `ms` and includes `σ:`.
  - Run `--deep-stats` without explicit `--time-unit` on a fast command, verifying unit scaling works.
- Check formatting and clippy.
