# Spec: Exclude Fork/Spawn Overhead on Linux (Post-Fork Timing) (#81)

## Problem / Motivation
References: `sharkdp/hyperfine#814`, `#81`.

Previously, joulex started the wall-clock timer immediately before `command.spawn()`. For a large parent process (such as a CLI runner with large memory allocations, open files, or complex thread runtime state), `fork()`/`clone()` plus Rust `Command` setup costs between 100 µs and 1 ms.

For micro-benchmarks of fast CLIs (e.g. `joulex -N true` where executions take under 5 ms), this spawn overhead accounts for a large fraction of the reported execution time. While shell calibration (`ShellExecutor::calibrate`) subtracts the intermediate shell's startup cost, it does not remove joulex's own process spawning overhead in `-N` / `--no-shell` mode.

## Contract & Architecture

On Linux (`target_os = "linux"`):
1. **Pipe Signaling with `O_CLOEXEC`:**
   - Before spawning the child process, a pipe is created using `libc::pipe2(..., libc::O_CLOEXEC)`. Both read and write ends are managed with RAII `OwnedFd` handles.
   - A `command.pre_exec` hook references the write end, ensuring the descriptor is inherited into the child process after fork.
2. **Exec-Time Synchronization:**
   - The child process is spawned (`command.spawn()`).
   - The parent closes its copy of the write end (`drop(write_fd)`).
   - The parent performs a blocking 1-byte read on `read_fd`.
   - In the child, after all pre-exec hooks (affinity, priority, process group) finish, `execve` replaces the process image.
   - Upon `execve`, the Linux kernel automatically closes all file descriptors marked with `O_CLOEXEC`.
   - The parent's read unblocks and returns `0` (EOF).
3. **Immediate Post-Exec Timing:**
   - The parent calls `WallClockTimer::start()` immediately upon unblocking.
   - Timing measures the command's execution starting directly from the kernel `execve` transition, excluding fork and spawn overhead.
4. **Non-Linux Platforms:**
   - On macOS and Windows, standard pre-spawn timing is retained, ensuring zero overhead and preserving native `posix_spawn` optimizations on Darwin.

## Verification
- Unit and integration tests pass on all targets.
- Clippy passes with no warnings across `--all-targets`.
