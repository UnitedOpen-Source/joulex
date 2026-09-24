# Specification: CPU Utilization Metric and Off-CPU Time Detection

**Issue:** [#21](https://github.com/UnitedOpen-Source/joulex/issues/21)  
**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** In Progress  
**Upstream References:** `sharkdp/hyperfine#898`, `sharkdp/hyperfine#711`, `Gabriella439/bench#25`, `Gabriella439/bench#26`, `sharkdp/hyperfine#781`, `sharkdp/hyperfine#882`

---

## 1. Overview & Context

In command-line benchmarking, measuring wall-clock time alone is insufficient to diagnose the root cause of performance variations. A command may appear slow because it performs intensive computation on multiple CPU cores, or because it is blocked off-CPU waiting for I/O, timers (`sleep`), thread locks, or page faults.

Currently, `joulex` records real (wall), user CPU, and system CPU times, but does not calculate or display CPU utilization in the benchmark summary, nor does it warn when a command is predominantly off-CPU.

Furthermore, upstream `Gabriella439/bench` defines `--before` and `--after` for per-iteration setup and teardown, which directly correspond to `hyperfine`'s `--prepare` and `--conclude`. Adding visible aliases for `--before` and `--after` makes `joulex` a drop-in superset for both ecosystems.

## 2. Functional Specification

### 2.1 CPU Utilization Percentage
- **Formula:**
  $$\text{CPU \%} = \frac{\bar{t}_{\text{user}} + \bar{t}_{\text{system}}}{\bar{t}_{\text{real}}} \times 100\%$$
- When $\bar{t}_{\text{real}} > 0$:
  - Format as `, CPU: {cpu_pct:.0}%` (e.g. `, CPU: 98%`, `, CPU: 390%` for multi-threaded, or `, CPU: 0%` for sleeping/waiting).
  - Placed in terminal output inside the bracket block:
    `[User: 1.2 ms, System: 0.8 ms, CPU: 85%, Peak Memory: 8.4 MB]`.
- Store `cpu_percent: Option<f64>` in `BenchmarkResult` (serialized in JSON export when present, omitted when `None`).

### 2.2 Substantial Off-CPU Time Warning
- In `src/output/warnings.rs`, add variant:
  `Warnings::OffCpuTime(Second, Second, f64)` representing `(wall_time, cpu_time, ratio)`.
- **Detection Criteria:**
  - $\bar{t}_{\text{real}} \ge 0.100\text{ s}$ (100 ms floor to avoid noise/granularity artifacts at microsecond scale).
  - $\bar{t}_{\text{real}} \ge 5.0 \times (\bar{t}_{\text{user}} + \bar{t}_{\text{system}})$ (command was off-CPU for $\ge 80\%$ of elapsed time).
- **Formatted Warning Message:**
  `"Substantial off-CPU time detected: the process spent most of its wall-clock time waiting (I/O, sleep, locks, or system scheduling) rather than executing on-CPU (wall: {wall}, CPU: {cpu}, ratio: {ratio:.1}x)."`
- **Suppression:**
  Suppressed when `--suppress-outlier-warnings` is specified.

### 2.3 `bench` Setup/Teardown Aliases
- In `src/cli.rs`:
  - Add `.visible_alias("before")` to `--prepare`.
  - Add `.visible_alias("after")` to `--conclude`.

### 2.4 Verification of Iteration Environment Variables
- Ensure integration tests assert that `JOULEX_ITERATION` and `HYPERFINE_ITERATION` are accessible in `--prepare`, the benchmarked command, and `--conclude` across warmup and benchmark runs.

---

## 3. Test Plan

1. **Unit Tests:**
   - Test warning string formatting for `Warnings::OffCpuTime`.
   - Test `BenchmarkResult` JSON serialization with `cpu_percent`.
2. **Integration Tests:**
   - Test that `--before` works identically to `--prepare`.
   - Test that `--after` works identically to `--conclude`.
   - Test that off-CPU warning appears on `sleep 0.1` and is suppressed with `--suppress-outlier-warnings`.
   - Test that `JOULEX_ITERATION` and `HYPERFINE_ITERATION` are expanded properly during prepare, main, and conclude.
