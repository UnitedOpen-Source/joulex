# Spec: `--priority normal|high|idle|realtime` (#80)

## Problem
Scheduler interference (preemption by other processes) is a major noise source.
Running the benchmarked process with a higher priority reduces it. A lower
priority is useful for benchmarking background workloads.

## Contract
```
--priority <POLICY>   normal (default) | high | idle | realtime
```
| Policy | Linux | macOS | Windows |
|---|---|---|---|
| normal | inherited | inherited | inherited |
| high | `setpriority(-20)` | `setpriority(-20)` | `HIGH_PRIORITY_CLASS` |
| idle | `SCHED_IDLE` | `setpriority(19)` | `IDLE_PRIORITY_CLASS` |
| realtime | `SCHED_FIFO`, max priority | rejected at option parsing | `REALTIME_PRIORITY_CLASS` |

- Applies to every process joulex starts: the benchmarked command, `--prepare`,
  `--conclude`, `--setup`, `--cleanup`, and the shell spawning time calibration,
  so the subtracted shell overhead is measured under the same priority.
- Unix: set in the child between fork and exec (`pre_exec`, like `--affinity`),
  so the program and all its children inherit it. Windows: `SetPriorityClass`
  on the still-suspended process, before it is resumed.
- `high`/`realtime` without privileges fail with a hint: on Linux, root or
  `sudo setcap cap_sys_nice+ep "$(command -v joulex)"` (root in a container
  usually lacks `CAP_SYS_NICE`); on macOS, root. Windows silently turns
  `realtime` into `high` without administrator rights.
- `realtime` prints a starvation warning at startup (`--timeout`, #51, does not exist yet).
- The calibration error now keeps its cause, so the hint (or e.g. "No such file
  or directory" for a missing `--shell`) is shown instead of a generic message.

## Verification
- Linux (Docker, Rust 1.88) with `--cap-add SYS_NICE`: the benchmarked process
  reports policy 0 / 5 (SCHED_IDLE) / nice −20 / policy 1 (SCHED_FIFO) for
  normal / idle / high / realtime (`/proc/$$/stat`). Without the capability,
  `high` and `realtime` fail with the hint.
- macOS: idle → nice 19; high without root → the hint; realtime → rejected.
- Windows: `SetPriorityClass` is type-checked with cross-target clippy only.
