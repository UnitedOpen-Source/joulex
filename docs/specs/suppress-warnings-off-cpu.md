# Spec: Decouple Off-CPU Warning Suppression from Outliers (Issue #112)

## 1. Problem Statement
1. In PR #23, `Warnings::OffCpuTime` was added to detect benchmarks spending most of their wall-clock time off-CPU (wall ≥ 100 ms and wall ≥ 5 × CPU time).
2. However, it was coupled to `--suppress-outlier-warnings`, meaning:
   - Legitimate I/O-bound benchmarks (e.g. `curl`, `sleep`, database queries) could only suppress off-CPU warnings by hiding statistical outlier warnings too.
   - The flag name `--suppress-outlier-warnings` was unrelated to off-CPU behavior.
3. The warning text ("rather than executing on-CPU") sounded alarming for commands where waiting is the intended behavior.

## 2. Technical Design
1. **Options Decoupling (`src/options.rs`):**
   - Add `pub suppress_off_cpu_warnings: bool` to `Options` (default `false`).
   - Keep `pub suppress_outlier_warnings: bool` for outlier warnings (`SlowInitialRun`, `OutliersDetected`).
   - Add CLI argument `--suppress-warnings <KIND,...>` (alias `--no-warning <KIND>`), accepting `off-cpu`, `outliers`, `all`.
   - Add `--no-off-cpu-warning` flag as a direct toggle for suppressing off-CPU warnings.
   - If `--suppress-outlier-warnings` is specified, it sets `suppress_outlier_warnings = true` (outliers only).
2. **Benchmark Warning Emission (`src/benchmark/mod.rs`):**
   - Guard `Warnings::OffCpuTime` with `!self.options.suppress_off_cpu_warnings`.
   - Guard `Warnings::SlowInitialRun` and `Warnings::OutliersDetected` with `!self.options.suppress_outlier_warnings`.
3. **Clarify Warning Message (`src/output/warnings.rs`):**
   - Explain that off-CPU time is normal for I/O-bound commands, but indicates external latency is included in the measurements.
4. **Documentation Updates:**
   - Update `src/cli.rs` `--help`.
   - Update `doc/joulex.1`.
   - Update `doc/understanding-output.md`.

## 3. Verification Plan
- Unit tests for option parsing:
  - `--no-off-cpu-warning` sets `suppress_off_cpu_warnings = true`.
  - `--suppress-warnings=off-cpu` sets `suppress_off_cpu_warnings = true` and `suppress_outlier_warnings = false`.
  - `--suppress-warnings=outliers` sets `suppress_outlier_warnings = true` and `suppress_off_cpu_warnings = false`.
  - `--suppress-warnings=all` sets both to `true`.
  - `--suppress-outlier-warnings` sets `suppress_outlier_warnings = true` and `suppress_off_cpu_warnings = false`.
- Integration tests:
  - An I/O-bound benchmark with `--no-off-cpu-warning` suppresses the off-CPU warning.
  - An I/O-bound benchmark with `--suppress-outlier-warnings` still emits the off-CPU warning.
  - An I/O-bound benchmark with `--suppress-warnings off-cpu` suppresses the off-CPU warning.
