# Benchmark regression gate in CI

joulex can compare a run with a baseline exported earlier and fail when a
benchmark got **significantly** slower:

```sh
joulex --export-json baseline.json 'target/release/app input.txt'        # on main
joulex --compare baseline.json --fail-if-regressed 5% \
       --export-diff-markdown diff.md 'target/release/app input.txt'     # on the PR
```

```
Comparison with baseline.json
  Command       Baseline        Current    Change  Significance
  app      28.2 ms ± 2.6  50.1 ms ± 2.7  +77.5% ▲  p < 0.001     REGRESSION
```

- Commands are matched by their displayed name (after parameter substitution, or
  the `-n` name). Unmatched commands are listed as `new` / `removed`.
- A benchmark regresses if it is slower than the baseline by more than the
  threshold **and** the difference is statistically significant (bootstrap Welch
  test on the per-run times, p < 0.05). Noise alone does not fail the gate. Both
  exports need per-run times (every joulex JSON export has them), and at least 3
  runs each.
- Exit codes: `0` ok, `3` regression. Any other non-zero code is an ordinary error.
- The exports and the comparison table are written before joulex exits with 3.

## GitHub Actions

```yaml
name: Benchmarks
on:
  pull_request:
jobs:
  bench:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      # Baseline: the target branch, built and measured on the same runner, so
      # that differences between runner machines don't count as regressions
      - run: |
          git worktree add ../base "origin/${{ github.base_ref }}"
          (cd ../base && cargo build --release)
          joulex --warmup 3 --export-json baseline.json -n app '../base/target/release/app input.txt'

      - run: cargo build --release
      - run: |
          joulex --warmup 3 --compare baseline.json --fail-if-regressed 5% \
                 --export-diff-markdown diff.md -n app 'target/release/app input.txt'

      - if: always()
        run: cat diff.md >> "$GITHUB_STEP_SUMMARY"
```

Measuring both versions **in the same job** matters: shared CI runners differ
from each other by far more than 5%. A baseline stored from an earlier run is
only meaningful on dedicated benchmark machines (see `--check-system=strict`).
The `-n app` name makes both runs match even though the command paths differ.
