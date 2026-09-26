# Spec: Better Progress UI (#66)

## Problem / Motivation
References: `sharkdp/hyperfine#416`, `#581`, `#670`, `#706`, `#820`, `joulex#66`.

When benchmarking commands that take several seconds or minutes per run:
1. During the initial time measurement, the progress bar shows a static message `"Initial time measurement"` with no counter and no indication of progress or elapsed time, appearing completely frozen.
2. During the subsequent benchmark runs, the message remains static, giving no feedback on the current running estimate (mean ± standard deviation) or energy measurements.
3. The progress bar displays a wide bar and ETA, but lacks a `{pos}/{len}` run count counter to show the exact iteration progress.

## Proposed Contract

### 1. Progress Bar Templates
- **Standard Run Template:**
  ```
   {spinner} {msg:<32} {wide_bar} {pos}/{len} ETA {joulex_eta} 
  ```
  Where `{pos}/{len}` displays the completed runs and total planned runs, and `{joulex_eta}` formats the custom decreasing ETA.
- **Initial Time Measurement Template:**
  ```
   {spinner} {msg} {elapsed_precise}
  ```
  Ticks elapsed time (`00:01:23`) every 80ms (Unix) / 200ms (Windows) so users see active progress while the initial run executes.
- **Template Switching Helpers:**
  - `create_progress_template(msg_template: &str) -> String`
  - `replace_message_template(bar: ProgressBar, template: &str) -> ProgressBar`
  - `reset_progress_template(bar: ProgressBar) -> ProgressBar`

### 2. Live Estimate Messages
- After each completed benchmark iteration:
  - When $n = 1$:
    `Current estimate: <mean>` (with energy if `--energy`: ` · <energy> J`)
  - When $n \ge 2$:
    `Current estimate: <mean> ± <stddev>` (with energy if `--energy`: ` · <energy> J ± <energy_sd> J`)
- Colors: mean in green, stddev in cyan, energy in yellow (when color is enabled).
- Applied consistently to:
  - Grouped execution in `Benchmark::run` (`src/benchmark/mod.rs`).
  - Round-robin execution in `Scheduler::run_benchmarks` (`src/benchmark/scheduler.rs`).

### 3. Style Compatibility
- `OutputStyleOption::Disabled`, `Basic`, and `Color`: no progress bars rendered. Hidden progress bars safely ignore style updates and message changes without overhead or visual output.

## Verification Plan
- Unit tests:
  - Progress bar template generation and message formatting.
  - Template switching and resetting with `replace_message_template` and `reset_progress_template`.
  - Progress estimate message formatting (single run, multiple runs with stddev, with and without energy).
  - ETA calculation monotonic decrease and overflow protection.
- Integration tests:
  - Progress bar appearance during benchmark runs with `--style full`.
  - Output suppressed or clean with `--style basic` and `--style disabled`.
