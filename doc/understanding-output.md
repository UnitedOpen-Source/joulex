# Understanding joulex output

This page explains every field joulex prints, how the numbers are computed, and how to get
reliable results. See the [README](../README.md) for installation and basic usage.

- [Anatomy of a benchmark report](#anatomy-of-a-benchmark-report)
- [Warnings](#warnings)
- [The summary and relative speed](#the-summary-and-relative-speed)
- [What runs when: setup, prepare, conclude, cleanup](#what-runs-when-setup-prepare-conclude-cleanup)
- [Benchmarking across git branches](#benchmarking-across-git-branches)
- [Reducing noise](#reducing-noise)

## Anatomy of a benchmark report

```
$ joulex -N -w 2 -r 20 --deep-stats 'sleep 0.05' 'sleep 0.1'
Benchmark 1: sleep 0.05
  Time (mean ± σ):      58.1 ms ±   2.0 ms    [User: 1.2 ms, System: 1.5 ms, CPU: 5%, Peak Memory: 1.0 MB]
  Range (min … median … max):    55.0 ms …  57.8 ms …  62.0 ms    20 runs
  Bootstrap 95% CI:   [mean: 57.2 ms … 59.0 ms, median: 56.8 ms … 59.6 ms, σ: 1.5 ms … 2.4 ms]
  Percentiles:        [p05: 55.4 ms, p25: 56.4 ms, p75: 59.7 ms, p95: 61.2 ms (IQR 3.3 ms), geometric mean: 58.1 ms]
```

| Field | Meaning |
|---|---|
| `Time (mean ± σ)` | Arithmetic mean of the **wall-clock** time of all timed runs, and the sample **standard deviation** σ of those runs. σ describes the spread of *individual runs*; it is **not** the uncertainty of the mean (that is roughly σ/√n; see the bootstrap CI below). When joulex runs commands through a shell, the calibrated shell start-up time is subtracted. |
| `Time (abs ≡)` | Shown instead of mean ± σ when only a single run was performed. |
| `User` / `System` | Mean CPU time per run spent in user mode and in the kernel, **summed over the process and all of its children**. They are CPU-time components and don't need to add up to the wall time (see below). |
| `CPU` | `(User + System) / mean wall time`. Above 100% means the command used several cores in parallel (e.g. 380% ≈ 3.8 cores busy); well below 100% means the command mostly waited (I/O, sleep, locks, network). |
| `Peak Memory` | Largest maximum resident set size (RSS) of a single run, as a human-readable size. On Unix it is measured per run for the benchmarked process and the processes it waited for (e.g. the command started by the shell); `--prepare`/`--setup` commands are not included. |
| `Range (min … median … max)` | Fastest, median and slowest timed run, and the number of runs (warmup runs are not counted). The median is more robust than the mean when a few runs were disturbed. |
| `Bootstrap 95% CI` | Only with `--deep-stats`: 95% confidence intervals for the **mean**, the **median** and **σ**, computed by bootstrapping (5,000 resamples). If you repeated the whole benchmark many times, about 95% of such intervals would contain the true value. |
| `Percentiles` | Only with `--deep-stats`: the 5th, 25th, 75th and 95th percentile of the run times (linear interpolation), the interquartile range `IQR = p75 − p25`, and the geometric mean. p95 shows how bad the slow runs get; the IQR is a spread measure that ignores outliers. The JSON export always contains `percentiles` and `geometric_mean`. |
| `Resources (mean)` | Only with `--resource-usage` (Unix): mean **voluntary** context switches (the process blocked: I/O, locks, sleep), **involuntary** ones (it was preempted: a busy machine or too many threads), **minor/major page faults** (major = needed disk I/O) and **block I/O operations**. Useful to explain *why* two commands differ. Per-run values are exported as `resources` in JSON. With the default shell the counters include the shell process; use `-N` to exclude it. |
| `Energy (mean)` / `Power` | Only with `-E/--energy` on Linux with readable RAPL counters: mean energy per run in joules, and average power (energy / mean time) in watts. The counters measure the **whole CPU package**, including idle and background power, not just the benchmarked process. If the counters are unavailable (other operating systems, missing permissions), joulex prints `RAPL unprivileged/unavailable on host`; on most distributions `energy_uj` is root-only since CVE-2020-8694. |

### Why can `User + System` be lower or higher than the wall time?

- **Lower** (low `CPU %`): the process was waiting, e.g. `sleep`, disk or network I/O, lock
  contention, or waiting for another process (a server, a daemon) that does the actual work.
  CPU time spent in *other* processes is not attributed to the command.
- **Higher** (`CPU %` > 100%): several threads or child processes ran in parallel.
- **Near zero for very fast commands**: the OS accounts CPU time with limited resolution
  (clock ticks), so commands of a few milliseconds may show `0.0 ms` or rounded values.

## Warnings

| Warning | What it means / what to do |
|---|---|
| *Command took less than 5 ms to complete* | Shell start-up correction can't be more precise than this. Use `-N`/`--shell=none` to avoid the shell entirely. |
| *The first benchmarking run … was significantly slower* | Caching effects (file system cache, JIT, lazy loading). Use `--warmup N` to benchmark a warm state, or `--prepare` to reset caches before *every* run for a cold state. |
| *Statistical outliers were detected* | A few runs were far from the median (modified Z-score). Something else was using the machine; see [Reducing noise](#reducing-noise). Silence it with `--suppress-outlier-warnings`, or exclude them from the statistics with `--discard-outliers` (at most 5% of the runs, but at least one; the discarded run numbers are exported as `discarded_outliers`). |
| *Too many of the N runs … look like outliers, so none were discarded* | `--discard-outliers` refused to drop more than 5% of the runs: the distribution is probably multimodal (two clusters of run times), and dropping runs would hide that. |
| *Substantial off-CPU time detected* | Wall time ≥ 5 × CPU time: the command mostly waited. That's expected for I/O-, network- or sleep-bound commands, but it means you are measuring the environment as much as the program. |
| *Ignoring non-zero exit code* | `-i`/`--ignore-exit-code` is active and some runs failed. With `--omit-failed-runs`, failed runs are excluded from the statistics. |

## The summary and relative speed

```
Summary
  sleep 0.05 ran
    1.87 ± 0.06 times faster than sleep 0.1 (109.4 ms, +51.0 ms)
      [Bootstrap t-test: t = -93.74, p = 0.0000 -> statistically significant (p < 0.01)]
```

- The fastest command (or the `--reference` command) is the baseline. Each ratio is the
  slower mean divided by the faster one, so it is always ≥ 1; the words *faster*/*slower*
  give the direction.
- The parentheses show the other command's mean and the absolute difference to the baseline
  (`+` = slower than the baseline, `−` = faster), in the unit of the largest mean, plus mean
  energy and its difference when `--energy` is available.
- The `±` is propagated from both standard deviations, assuming independent measurements:
  `σ_r = r · √((σ_a/μ_a)² + (σ_b/μ_b)²)`. Because σ describes single runs, this is a
  conservative spread of the ratio, not a confidence interval.
- With `--deep-stats`, a two-sample bootstrap test compares the per-run times. `p < 0.05`
  means a difference this large would rarely appear by chance if both commands were
  equally fast. It says nothing about whether the difference *matters*; look at the ratio
  for that. With very few runs, prefer more runs over reading much into the p-value.

## What runs when: setup, prepare, conclude, cleanup

| Option | Runs | Timed? |
|---|---|---|
| `-s`, `--setup CMD` | once **before all runs** of a benchmark (after the parameters are substituted); give it once for all commands or once per command | no |
| `-w`, `--warmup N` | N untimed runs of the command; with `--warmup auto`, runs until the last 5 differ by at most 1% (at most 100; the count is printed and exported as `warmup_runs`) | no |
| `-p`, `--prepare CMD` (alias `--before`) | before **each** run (warmup and timed) | no |
| the command | each timed run | **yes** |
| `-C`, `--conclude CMD` (alias `--after`) | after **each** run | no |
| `-c`, `--cleanup CMD` | once **after all runs** of a benchmark; once for all commands or once per command | no |

`JOULEX_ITERATION` (and `HYPERFINE_ITERATION`) is set to the run index (`0`, `1`, … or
`warmup-0`, …) for the command, `--prepare` and `--conclude`. The same value is available as
the `{iteration}` placeholder, which also works with `-N`/`--shell=none`, e.g.
`joulex -N --prepare 'mkdir -p out/{iteration}' 'tool --out out/{iteration} input'`. It is
not expanded in `--setup`/`--cleanup`, which don't belong to a single run. The complete flow is shown in
[execution-order.png](execution-order.png).

With `--reference`, the reference command is benchmarked first, with the same
setup/prepare/conclude/cleanup handling as every other command.

## Benchmarking across git branches

**Option A: one worktree per branch** (fast; no rebuild between runs; works with uncommitted
changes):

```sh
git worktree add ../app-main main
(cd ../app-main && cargo build --release)
cargo build --release
joulex -N -w 3 \
  -n main '../app-main/target/release/app input.txt' \
  -n HEAD './target/release/app input.txt'
```

**Option B: switch and build in `--setup`**, one benchmark per branch:

```sh
joulex -L branch main,feature \
  --setup 'git switch {branch} && cargo build --release' \
  './target/release/app input.txt'
```

(Don't combine this with `-N`: `--setup`, `--prepare` and friends then run without a shell
too, and `&&` would be passed to `git` as a literal argument.)

Option B only works with the default (grouped) schedule: `--schedule round-robin` rejects a
parametrized `--setup`, because all setups would run before the interleaved runs and every
variant would be measured on the last branch built.

To compare against a saved baseline instead, export it once (`--export-json main.json`) and
later combine it with a live run: `joulex --import-json main.json './target/release/app input.txt'`.

## Reducing noise

1. **Warm up or reset caches deliberately**: `--warmup N` for warm-cache numbers;
   `--prepare 'sync; echo 3 | sudo tee /proc/sys/vm/drop_caches'` (Linux) for cold-cache numbers.
2. **Skip the shell** for fast commands: `-N`/`--shell=none`.
3. **Interleave commands** with `--schedule round-robin` so slow drifts (thermal throttling,
   background jobs, turbo budget) affect all commands equally.
4. **Quiet machine**: close browsers, IDEs and sync clients; stay on **AC power**; don't run
   other benchmarks or builds in parallel.
5. **Fix the CPU frequency** on Linux:
   `sudo cpupower frequency-set -g performance`, and optionally disable turbo boost
   (`echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo`).
6. **Pin to a core** to avoid migrations (especially on hybrid P/E-core CPUs):
   `joulex --affinity 2 ./app` (Linux and Windows; also accepts lists like `0,2-3`).
7. **More runs**: `--min-runs`/`--runs`. The uncertainty of the mean shrinks with √n; use
   `--deep-stats` to see the confidence interval.
8. **Energy measurements** are the most sensitive to all of the above, because the RAPL
   counters include everything the CPU package does. Prefer longer runs (≥ 100 ms), a quiet
   machine, and compare commands with `--schedule round-robin`.
