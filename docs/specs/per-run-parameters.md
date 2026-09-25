# Spec: per-run parameters — `--parameter-sample` (#57) and `--aggregate-parameter-runs` (#74)

## Problem
`-L name a,b,c` creates one benchmark per value. Two common needs are the opposite:
- **#57:** a *representative workload*: each run uses a different input, drawn at
  random, and commands are compared across the whole input set.
- **#74:** each value is *one sample of the same workload* (20 files to copy):
  run through all values and report one pooled mean ± σ per command template.

## Design: one mechanism
Parameters substituted **per run** instead of per benchmark
(`command::PerRunParameters`: variables + `PerRunMode::{Cycle, Random { seed }}`).
`Command::for_iteration(&BenchmarkIteration)` substitutes `{iteration}` and the
per-run values. It is called by every executor (including the `--debug-mode` mock)
where `with_iteration` was called before.

- **Cycle** (`--aggregate-parameter-runs`): run *i* uses combination *i mod n* of
  all `-L`/`-P`/`--parameter-file` variables (mixed radix, the last variable changes
  fastest). The run count is rounded up to whole cycles
  (`BenchmarkRunner::whole_cycles`, also in round-robin mode), so every value is
  used equally often: `--runs 1` with 20 values gives 20 runs. One benchmark per
  command template. The combination count is bounded by `--max-benchmarks`
  (100 000), which bounds the run count.
- **Random** (`--parameter-sample VAR VALUES`, repeatable; `--seed`, default 0): run
  *i* draws each variable from `StdRng(seed ^ mix(i) ^ warmup-bit)`. The draw
  depends only on the seed and the run index, so **every command sees the same
  values in the same order** (paired comparison), and runs are reproducible.
  Warmup runs use a separate stream. It combines with `-L`/`-P` (they still create
  separate benchmarks) and with `--aggregate-parameter-runs`.
- Display: the template stays unsubstituted, plus `(aggregated over N parameter
  values)` / `(sampled from N values)`.
- `--prepare`/`--conclude` get the same per-run values as their run.
  `--setup`/`--cleanup` run once per benchmark: using a per-run parameter there is
  an error (it would otherwise be passed on literally as `{name}`).
- Errors: `--aggregate-parameter-runs` without parameters; `iteration` as a
  sampled name (reserved); duplicate names across `-L`/`-P`/`-F`/sample.

## Not included
Per-run values in the JSON (which value each run used) and per-value statistics:
#192.

## Tests
- Unit (`command::per_run_tests`): cycle order, wrap-around, `runs_multiple`, no
  values for setup runs, names; random reproducibility, pairing across commands,
  seed and warmup streams, coverage of all values; prepare/run agreement.
- Integration (`tests/per_run_parameter_tests.rs`): the upstream test (`-P i 1 3`,
  `--runs 1` → times 1.123/2.123/3.123, mean 2.123, σ 1); whole cycles with two
  templates; name and errors; pairing over 30 runs; seed; combination with `-L`;
  name errors; setup/cleanup rejection.
