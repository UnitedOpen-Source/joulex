# Spec: `--until <TEXT>` / `--ready-when` — Benchmark Process Startup Readiness (#79)

## Problem / Motivation
References: `sharkdp/hyperfine#814`, `#79`.

Servers, daemons, language servers, and dev servers (e.g. `next dev` → "Ready in …", `postgres` → "ready to accept connections", web servers → "Listening on port …") do not exit on their own. However, **startup time / time-to-readiness** is one of the most critical performance metrics developers need to measure and optimize.

Currently, benchmarking startup latency requires custom, fragile shell scripts involving background processes, polling loops, or `curl` retries. These wrappers introduce non-trivial process spawning and polling overhead, and often leave orphaned background processes on failure or cancellation.

## Contract

```
--until <TEXT>     Stop the timer as soon as the command writes TEXT to stdout
                   (or stderr with --until-stderr), then terminate the process tree.
                   A run that exits without printing TEXT fails.
                   (visible alias: --ready-when)

--until-stderr     Match the pattern specified by --until against stderr instead of stdout.
                   Requires --until.
```

### 1. CLI Validation & Conflicts
- `--until <TEXT>` takes a non-empty string argument. An empty string is rejected with `OptionsError::EmptyUntilPattern`.
- `--until-stderr` is a boolean flag that requires `--until`.
- `--until` conflicts with `--show-output` (`-d`), `--output` (`-O`), and `--show-output-on-failure` because stdout and stderr must be managed and monitored by joulex's stream listener.

### 2. Streaming Matching & Boundary Safety
- Output is read in 64 KiB chunks from the monitored stream (`stdout` by default, `stderr` with `--until-stderr`).
- To prevent missing matches that cross 64 KiB chunk boundaries, a carry buffer preserves the last `needle.len() - 1` bytes across reads.
- When `read_until` detects `needle`, it returns `Ok(true)` immediately.

### 3. Immediate Timer Halt & Process Tree Termination
- The wall-clock timer is stopped **immediately** when the match is detected, excluding the time required to tear down the process.
- **Unix:**
  - Benchmark processes are placed in their own process group (`command.process_group(0)`).
  - Upon matching, `SIGTERM` is delivered to the entire process group `-(pid as libc::pid_t)`.
  - A fallback watchdog waits up to 2 seconds for clean exit; if the process does not exit, it delivers `SIGKILL` to prevent hanging processes.
- **Windows:**
  - The process is launched in an isolated Win32 Job Object.
  - Upon matching, `TerminateJobObject(job_handle, 1)` terminates the entire process tree.

### 4. Calibration & Non-Benchmark Isolation
- `ShellExecutor::calibrate()` does NOT use `--until`. Shell startup calibration measures empty shell execution time and must run to normal completion.
- Setup (`--setup`), cleanup (`--cleanup`), prepare (`--prepare`), conclude (`--conclude`), and baseline (`--subtract`) commands do not use `--until`.
- `--until` only applies to warmup runs and timed benchmark runs (`BenchmarkIteration::Warmup` and `BenchmarkIteration::Benchmark`).

### 5. Execution & Failure Semantics
- If the pattern is matched: the run is marked successful (`ExitStatus` set to 0).
- If the child process exits or closes the pipe **without** printing the pattern:
  - The run is marked as failed.
  - If the process exited with code 0 without printing the pattern, its status is synthesized to non-zero (exit code 1).
  - The failure error message clearly identifies: `Command exited without matching '--until' pattern in <when>`.
  - Can be ignored using `-i` / `--ignore-failure` / `--ignore-exit-code`.

## Verification Plan
- Unit tests:
  - `read_until` with needles split across chunk boundaries (e.g. 1+rest, rest+1, exact half).
  - `read_until` stream EOF without match.
  - `read_until` repeated partial matches followed by exact match.
  - CLI option parsing: empty pattern rejection, `--until-stderr` without `--until` rejection, conflicts with `--show-output`/`--output`/`--show-output-on-failure`.
- Integration tests:
  - Fast readiness detection: command that prints `READY` and sleeps 30s terminates in ~0.2s.
  - `--until-stderr` matching against stderr.
  - Process that exits without printing pattern fails with clear error.
  - Process with `-i` ignores failures when pattern is not printed.
  - Compatible with `--timeout`.
