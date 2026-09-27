# Spec: Exclude Fork/Spawn Overhead on Linux (Post-Fork Timing) (#81)

## Problem / Motivation
References: `sharkdp/hyperfine#814`, `#81`.

Previously, joulex/perfratio measured execution time from before `command.spawn()` or attempted to synchronize using an `O_CLOEXEC` pipe unblocking in the parent after `execve`.

However, starting the timer in the parent after reading EOF from a CLOEXEC pipe severely underestimates short executions. When the child process calls `execve`, the kernel replaces the child's image and schedules the child. For fast commands (e.g. `true`, `sleep 0.001`, or micro-benchmarks taking < 5 ms), the child may finish or execute most of its work while the parent is still waking up from `read()` or context switching. When the parent finally starts its timer, the elapsed time reported is artificially close to zero or drastically undercounted.

Conversely, measuring from before `command.spawn()` includes parent process fork/clone and `Command` environment setup overhead (100 µs to 1 ms+), which inflates micro-benchmarks.

## Contract & Architecture

On Linux (`target_os = "linux"`):

1. **Child Pre-Exec Timestamp Acquisition:**
   - Before spawning the child process, a communication pipe is created using `libc::pipe2(..., libc::O_CLOEXEC)`. Both read and write descriptors are wrapped in RAII `OwnedFd` handles.
   - A `command.pre_exec` closure is registered. Because `pre_exec` hooks run in registration order, this timing hook executes after any user-specified affinity (`sched_setaffinity`) and priority (`setpriority`/`sched_setscheduler`) hooks, immediately before `execve`.
   - In `pre_exec`, the child calls `clock_gettime(libc::CLOCK_MONOTONIC, &mut ts)`.
   - The timestamp (`ChildTimestamp` containing `tv_sec: i64`, `tv_nsec: i64`) is transmitted through the pipe write descriptor using a robust loop handling partial writes and `EINTR`.
   - The write descriptor is then closed in the child, and the closure returns `Ok(())` so the kernel can invoke `execve`.
   - **Async-Signal Safety:** Only async-signal-safe syscalls (`clock_gettime`, `write`, `close`, `*__errno_location()`) are called within `pre_exec`. No heap allocations, locks, or formatting functions are invoked.

2. **Parent Protocol & Timing Synchronization:**
   - The parent starts a fallback/total wall-clock timer immediately before `command.spawn()`.
   - The parent spawns the child via `command.spawn()?`. In Rust's stdlib on Unix, `spawn()` unblocks once the child has either successfully executed `execve` or reported an exec failure.
   - Upon `spawn()` returning, the parent immediately drops its copy of the write descriptor and reads the `ChildTimestamp` from the read descriptor.
   - A robust read protocol handles partial reads, `EINTR`, unexpected EOF, and I/O errors. Because the child wrote the 16 bytes before calling `execve`, the payload is already in the kernel pipe buffer and read without blocking.
   - If pipe setup or reading fails, the parent gracefully falls back to the pre-spawn timer.

3. **Same-Clock End Measurement:**
   - When the child process terminates and is reaped via `wait4` (`unix_timer::wait_with_rusage`), or when `--until` matches its target pattern, the parent immediately samples the end timestamp using the exact same clock (`clock_gettime(libc::CLOCK_MONOTONIC, &mut end_ts)`).
   - Execution duration (`time_real`) is computed as `timespec_diff_seconds(&start_ts, &end_ts)`, clamped to non-negative values (`.max(0.0)`).

4. **Preservation of Total Time for Scheduling & ETA:**
   - The gross elapsed time including process spawn overhead (`wallclock_timer.stop()`) is preserved separately as `time_total` in `TimerResult` and `TimingResult`.
   - The benchmark runner uses `time_total` for `initial_total_time` so that benchmark scheduling (estimating iterations to satisfy `min_benchmarking_time`) and progress ETA accurately reflect the true round-trip wall-clock cost per run, while `time_real` accurately reflects pure execution time.

5. **Non-Linux Platforms:**
   - On macOS and Windows, standard pre-spawn timing is retained (`time_real == time_total == wallclock_timer.stop()`), ensuring zero behavioral change, zero regressions, and full compatibility.

6. **Energy interval:**
   - The RAPL sampler starts before `run_command_and_measure` and stops after it returns. On Linux, its interval therefore includes spawn and reap overhead, while reported `time_real` starts in the child before `exec`. Energy and reported wall time do not cover identical intervals; #59 tracks energy interval alignment.

## Verification
- Unit tests verify `ChildTimestamp` serialization, robust partial read handling, unexpected EOF detection, and nanosecond timespec arithmetic.
- Integration tests verify Linux execution timing, `--until` handling, and interaction with `--shell=none`, affinity, and priority.
- Clippy passes with no warnings across `--all-targets`.
- Format checks pass cleanly.
