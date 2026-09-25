# Spec: adaptive run count — `--target-precision` (#50)

## Contract
```
--target-precision PCT          run until the 95% CI half-width of the mean ≤ PCT of the mean
--max-benchmarking-time SECONDS time budget per command (default 60; requires --target-precision)
```
- Replaces the fixed run count (it conflicts with `--runs`). The runs stop when
  the target is reached, but never before `--min-runs` (default 10, at least 2)
  or in the middle of an `--aggregate-parameter-runs` cycle, and never after
  `--max-runs` or the time budget.
- `Range` line: `356 runs (±1.0% @95%, target 1%)`.
- Not reached: `Warning: The target precision of ±1.0% was not reached after 49
  runs (reached: ±10.8%). Increase '--max-benchmarking-time' or '--max-runs', or
  reduce the noise (see '--check-system').`
- Round-robin: each command keeps running until *it* reaches the target. The
  commands share the wall-clock time, so the budget is `max-benchmarking-time ×
  number of commands`.
- A first run excluded by `--first-run` does not count toward the precision.

## Statistics (`stats::precision`)
Half-width of the 95% CI of the mean relative to the mean: `t₀.₉₇₅(n−1) · s/√n /
mean`, with a Student t table (df 1–30 exact, then 2.021/2.000/1.980/1.960).

## Implementation
`BenchmarkRunner::needs_more_runs(started, budget)` is the stop rule. Both loops
(`Benchmark::run` for grouped, the interleaved loop in the scheduler for
round-robin) call it before each run instead of comparing with the planned count.
The progress bar grows as needed.

## Not included
The target and the precision reached are not in the JSON yet: the `Range` line and
the warning show them, and `mean`/`stddev`/`times` allow recomputing them.

## Tests
- Unit: t quantiles; the relative half-width (formula, constant samples, too
  few/zero values, narrowing with n).
- Integration (`tests/target_precision_tests.rs`): constant times stop at
  `--min-runs`; the report; whole cycles with aggregation; invalid values and
  conflicts; on unix with random 10–50 ms sleeps: `--max-runs` bound + warning,
  time budget bound, round-robin.
