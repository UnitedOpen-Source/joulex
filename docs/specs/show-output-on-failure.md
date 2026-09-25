# Spec: `--show-output-on-failure` (#96)

## Problem
By default the output of benchmarked commands is discarded. When a run fails, the
only way to see why is to re-run everything with `--show-output`, which prints
the output of every run, disturbs the measurement, and conflicts with `--style`.
Flaky failures (1 in 50 runs) are therefore hard to debug.

## Contract
```
--show-output-on-failure   capture the last 64 KiB of stdout and stderr of every run,
                           show them only when a run fails
```
- Failure (without `-i`): the error message ends with the last 20 lines of
  stderr, then stdout (`──── stderr (last N lines) ────`), or
  `(the command produced no output)`.
- With `-i`/`--ignore-failure`: each failed run's output is printed as a warning,
  at most 3 times per joulex invocation, then
  `the output of further failed runs is not shown`.
- Without the option, the failure message suggests it (next to `--show-output`).
- Conflicts with `--show-output` and `--output`; works with `--style`.
- Applies to benchmark and warmup runs (`--prepare` etc. keep their own policy).

## Safety
- The captured output is untrusted: control characters (ESC, BEL, …) are escaped
  with the sanitizer from #24, line by line.
- Lines longer than 300 characters (binary output) are cut to their **last**
  300 characters: the end of a line usually holds the error message.
- stdout is drained on the measuring thread and stderr on a second thread,
  concurrently, so a command that fills both pipes cannot deadlock (tested with
  5 MB on each stream). Only the last 64 KiB of each are kept (`read_tail`
  trims in batches, so the work is amortized O(n)).
- Reading through pipes (plus one thread spawn per run) can affect the
  measurement slightly, like `--output=pipe`. This is documented in `--help`.

## Implementation
`CommandOutputPolicy::CaptureTail` (both streams piped) → `execute_and_measure(…,
capture)` → `TimerResult::captured: Option<CapturedOutput>` (so `TimerResult` is no
longer `Copy`) → `run_command_and_measure_common` attaches it to the error, or
warns when the failure is ignored.

## Tests
- Unit: `read_tail` (keeps the last bytes); `tail_lines` (last 20 lines, escaping,
  long-line cut keeps the end, empty output).
- Integration (`tests/show_output_on_failure_tests.rs`): conflicts; the error shows
  stderr/stdout; without the option, the error suggests it; successful runs print
  nothing; with `-i` exactly the failed iteration is shown; at most 3 blocks; 5 MB on
  both streams without deadlock; terminal escape injection is neutralized.
