# Spec: `--timeout <DURATION>` per run (#51)

## Problem / Motivation
References: `sharkdp/hyperfine#576`, `#106`.

Benchmarking a matrix of implementations (e.g. `-L lang C,Rust,bash -L N 10,20,30,40`) often includes combinations that take minutes or hang indefinitely. Wrapping commands in external utilities like `timeout 2s` (GNU coreutils; not available on macOS or Windows by default) introduces 3–6 ms of process spawning overhead to every run, distorting fast benchmarks where precision matters most.

## Contract

```
--timeout <DURATION>   Kill a benchmark run that exceeds DURATION (e.g. 500ms, 2s, 1m).
                       The benchmark is marked as "timed out" and later runs of that command are skipped.
```

### 1. Duration Parsing
- Accepts standard duration units: `ns`, `us`, `µs`, `ms`, `s`, `m` / `min`, `h`.
- Bare positive numbers without unit are interpreted as seconds (e.g. `2` or `1.5`).
- Rejects zero, negative values, non-finite values (`inf`, `NaN`), and invalid formats with `OptionsError::InvalidTimeout`.

### 2. Process Tree Termination
- **Unix:** Benchmark processes are spawned in their own process group (`command.process_group(0)`). When the deadline expires, the watchdog thread delivers `SIGKILL` to `-(pid as libc::pid_t)` to kill the process and all descendant processes spawned by intermediate shells.
- **Windows:** The process is already created inside an isolated Win32 Job Object (`CreateJobObjectW`). When the deadline expires, the watchdog terminates the entire process tree via `TerminateJobObject(job_handle, 1)`.
- **Zero Overhead:** When `--timeout` is not specified, no watchdog thread is created, preserving default execution performance (~10 µs per run saved).

### 3. Execution & Failure Semantics
- `TimerResult` and `TimingResult` include `timed_out: bool`.
- Process termination induced by the watchdog is recognized by `run_command_and_measure_common`, preventing it from bailing with unexpected signal / non-zero exit code errors.
- **Intermediate Commands (`--prepare`, `--conclude`, `--setup`, `--cleanup`, `--subtract`):**
  If an intermediate command exceeds the timeout, the entire run immediately aborts with an actionable error:
  `The <type> command timed out (> <DURATION>).`
- **Benchmarked Commands:**
  - If a benchmark run (warmup, initial measurement, or timed iteration) times out:
    - The timed-out run is aborted and not recorded into `times_real`.
    - Subsequent runs for this command are skipped.
    - If earlier runs completed before the timeout, their measurements are retained in `times_real`.
    - If the timeout occurs on the first run (0 completed runs), aggregate metrics are set to the timeout duration (`mean = timeout`, `median = timeout`, `min = timeout`, `max = timeout`, `stddev = None`).
  - Terminal output:
    `  Timed out (> <DURATION>) — skipped remaining runs`
    (in warning yellow).
  - Statistical outlier warnings and off-CPU warnings are suppressed for timed-out benchmarks.

### 4. Summary & Relative Speed
- Timed-out benchmarks are excluded from ratio comparisons:
  `relative_speed` is `f64::NAN`, and they cannot be selected as the baseline/fastest reference.
- In `SortOrder::MeanTime` summary:
  Listed with `> <DURATION> (timeout)` preceding the command name.
- In `SortOrder::Command` relative speed table:
  Formatted with `> <DURATION> (timeout)`.

### 5. Exporters
- **JSON:**
  Contains `"timed_out": true`, `"timeout": <duration_in_seconds>`, plus whatever runs completed before the timeout in `"times": [...]` (empty array if timed out on first run).
- **CSV:**
  The `mean` column contains `><DURATION> (timeout)` (e.g. `>2.000 (timeout)`). Columns without measurements (when 0 runs completed) are left empty.
- **Markdown / AsciiDoc / OrgMode / HTML:**
  The `Mean` column shows `><DURATION> (timeout)`. The `Relative` column shows `n/a`.

## Verification
- Unit tests for duration parsing (`parse_duration`).
- Integration tests:
  - `--timeout 200ms -N 'sleep 5' 'sleep 0.01'` finishes in under 2 seconds. Command 1 is marked as timed out, command 2 runs normally.
  - Process group killing: on Unix, nested background processes (`sleep 5 & sleep 5; wait`) leave no orphan processes after timeout.
  - JSON export validation: verifies `timed_out: true`, `timeout: 0.2`.
  - CSV and Markdown export validation: verifies `>0.200 (timeout)` and `n/a`.
  - Intermediate command failure validation for `--prepare 'sleep 5'`.
