# Spec: `--first-run include|separate|discard` (#55)

## Problem
The first timing run of a command often includes one-time costs (page cache, lazy
loading, JIT). joulex only warns ("the first benchmarking run … was significantly
slower") and keeps the run in the statistics, so users can neither see the cold
start on its own nor get clean warm statistics without guessing a `--warmup` count.

## Contract
```
--first-run <MODE>   include (default) | separate | discard
```
| Mode | Statistics / per-run vectors | Output | JSON |
|---|---|---|---|
| `include` | first run included (unchanged behaviour) | unchanged | unchanged |
| `separate` | first run excluded | `Cold (1st run):` line before `Time`, `N runs (+1 cold)` | `first_run: {time, user, system, memory_usage_byte, energy_joules?, exit_code}` |
| `discard` | first run excluded | `N runs (first run discarded)` | no `first_run` |

- With `separate`/`discard`, **one extra run** is performed (`BenchmarkRunner::extra_runs`),
  so `--runs N` still yields N measured runs. It also applies to round-robin, where
  every runner gets `common + extra_runs()`.
- `separate` with `--warmup N`/`--warmup auto`: the first **warmup** run is the cold
  one. It is reported as `Cold (warmup 1):` with `first_run.warmup = true` (wall-clock
  time only, because warmup runs don't record user/system time). No extra run is
  performed and no timing run is excluded. `discard` with warmup still drops the
  first timing run.
- A benchmark that ends with a single run (e.g. interrupted by Ctrl-C) keeps it.
- Indices in `omitted_failed_runs`/`discarded_outliers` and `--export-runs` iteration
  numbers count from the first *non-cold* run.
- The Ctrl-C "interrupted after X of Y runs" counts exclude the extra run.

## Implementation
- `options.rs`: `FirstRunPolicy` + `Options::first_run`; `cli.rs`: `--first-run`.
- `benchmark/mod.rs`: the first run is recorded as before. `finish()` calls
  `take_first_run()`, which removes index 0 from all per-run vectors (`retain_runs`)
  before failed-run omission and outlier discarding.
- `benchmark_result.rs`: `FirstRun` and `BenchmarkResult::first_run` (serde default,
  skipped when `None`, so old JSON still imports).
- The slow-first-run warning suggests `--first-run=separate`.

## Tests (`tests/first_run_tests.rs`)
The default, `separate`, `discard`, `separate` + warmup, a single run, round-robin,
an invalid value, and a real cold start (unix: a marker file makes the first run
0.5 s slower; asserts it is reported and excluded from `max`).
