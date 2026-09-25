# Spec: Ctrl-C Timing Recovery & Graceful Interruption (Issue #89)

## 1. Problem Statement
In existing versions of hyperfine / joulex, pressing Ctrl-C during a benchmark terminates the process abruptly with exit code 130 / -2 (SIGINT):
- Any runs gathered so far for the benchmark currently in progress are discarded.
- Incomplete benchmarks produce no output, no relative-speed summary, and are omitted from export files.
- For long-running benchmarks (e.g. commands taking seconds/minutes or multi-repetition energy measurements), user interruption loses all accumulated timing data.

## 2. Technical Design

### 2.1 Interrupt Handling (`src/util/interrupt.rs`)
- Dependency: `ctrlc = { version = "3.4", features = ["termination"] }`.
- Maintain an atomic counter `static PRESSES: AtomicU8 = AtomicU8::new(0)`.
- On first Ctrl-C:
  - Increment counter to 1 (`interrupted() == true`).
  - Do not exit immediately; signal runner to stop starting new runs.
- On second Ctrl-C:
  - Counter >= 2: call `std::process::exit(130)` immediately.
- Functions:
  - `pub fn install() -> anyhow::Result<()>`: idempotent handler registration.
  - `pub fn interrupted() -> bool`: checks if counter > 0.
  - `#[cfg(test)] pub fn reset()`: resets counter to 0 for test isolation.
  - `#[cfg(test)] pub fn trigger()`: increments counter for test simulation.

### 2.2 Dedicated Error for Signal Interruption (`src/error.rs`)
- Add `#[derive(Debug, Error)] #[error("Benchmark interrupted by user")] pub struct Interrupted;`.
- In `src/benchmark/executor.rs`:
  - When a child process terminates with a non-zero exit status or signal AND `crate::util::interrupt::interrupted()` is true, bail with `crate::error::Interrupted`.

### 2.3 Benchmark Execution Loops (`src/benchmark/mod.rs` & `scheduler.rs`)
- **Grouped Schedule:**
  - Check `interrupted()` before setup, warmup, initial measurement, and each timed iteration.
  - Catch `Interrupted` error during timed iterations, break gracefully without dropping completed runs.
  - Discard the incomplete interrupted run.
  - If a benchmark has completed $\ge 1$ runs:
    - Compute statistics with $N$ completed runs (for $N=1$, $\sigma = \text{None}$).
    - Mark `res.runs_planned = Some(self.count)`.
    - Print `(interrupted after {t_num} of {count} runs)`.
  - If a benchmark has 0 completed runs:
    - Print note `(interrupted before completing any runs)`.
    - Do not record result in `results`.
  - Run cleanup for any benchmark that executed setup.
  - Break scheduler loop across commands; do not start subsequent unbenchmarked commands.
- **Round-Robin Schedule:**
  - Check `interrupted()` and break interleaved loop.
  - Run cleanup for all runners where setup executed.
  - Collect results for any runner with $\ge 1$ completed runs, marking `runs_planned`.
  - Skip runners with 0 completed runs with an informative note.

### 2.4 Relative Speed Comparison & Exports
- **Summary Title:**
  - If interrupted:
    - When unbenchmarked commands remain: `Summary (interrupted — N commands not benchmarked)`.
    - When all commands benchmarked partially/fully: `Summary (interrupted)`.
- **JSON Export (`src/export/json.rs`):**
  - Add `interrupted: bool` to `JoulexSummary` (`#[serde(default, skip_serializing_if = "std::ops::Not::not")]`).
  - Add `runs_planned: Option<u64>` to `BenchmarkResult` (`#[serde(default, skip_serializing_if = "Option::is_none")]`).
- **Exit Code:**
  - If `crate::util::interrupt::interrupted()` is true, exit with code 130 upon completion of summary and export.

## 3. Verification Plan
- Unit tests:
  - `interrupt::install()`, `interrupted()`, `reset()`.
  - JSON serialization of `interrupted: true` and `runs_planned`.
- Integration tests:
  - Unix process signaling: spawn joulex with `std::process::Command`, benchmark `sleep 0.1` for 50 runs, send `SIGINT` via `libc::kill`, and verify:
    - Exit code is 130.
    - Output contains `interrupted after` and `Summary (interrupted)`.
    - JSON export has `"interrupted": true`, `"runs_planned": 50`, and `1 <= times.len() < 50`.
  - Multi-command interruption: 2 commands where second is interrupted, verify summary contains comparison.
