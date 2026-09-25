# Spec: CI regression gate — `--compare`, `--fail-if-regressed`, `--export-diff-markdown` (#61)

## Contract
```
--compare FILE                 compare with a JSON export (matched by command name)
--fail-if-regressed PCT        exit 3 if a benchmark is slower by > PCT and p < 0.05 (requires --compare)
--export-diff-markdown FILE    the comparison as a Markdown table (requires --compare)
```
- The baseline is loaded (and validated, via `import::import_json`) **before**
  benchmarking, and so is the threshold: bad input fails fast.
- After the benchmarks and the regular exports, the table is printed (unless
  `--style none`), the Markdown file is written, and then joulex exits with 3
  if there is a regression. An interrupted run (Ctrl-C) never reports one.
- Rows: `Command | Baseline (mean ± σ) | Current | Change (+x.x% ▲/▼) |
  Significance (p) | verdict`, where the verdict is `REGRESSION` (threshold and
  significant), `slower` / `faster` (significant), or `~` (not significant).
  Then `new` and `removed` commands. Names are escaped (#24); in Markdown, `|`
  is escaped.

## Significance
`stats::deep::compare_samples` (bootstrap Welch test, fixed seed, so the result
is reproducible) on the per-run times; it needs ≥ 3 runs per side. When both
samples are constant with different means (e.g. `--debug-mode`), the difference
is certain (p = 0). Without per-run times on either side, the p-value is `n/a`
and the row can never be a regression.

## Non-goals (for now)
- `--metric` (#48) to gate on energy or memory instead of wall time.
- Baselines with several results of the same name: the last one wins.

## Tests
- Unit (`compare`): matching with new/removed; noise +10% not significant vs a
  clear +50% slowdown; thresholds; improvements are never regressions; constant
  samples and missing times; threshold parsing; exact Markdown; terminal row.
- Integration (`tests/compare_tests.rs`): unchanged → exit 0 with `~`; +100% →
  exit 3, `REGRESSION`, and a Markdown file with new/removed rows; 1000% threshold
  → exit 0; report-only without a threshold; a missing file, a bad threshold, and
  `--fail-if-regressed` without `--compare` all fail before benchmarking.
