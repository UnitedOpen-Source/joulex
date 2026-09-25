# Spec: interference diagnostics — trend, multimodality, inflated variance (#54)

## Problem
joulex flags individual outliers (modified Z-score) and a slow first run. It
misses three common kinds of interference:
1. **Trends:** run times rising or falling over the runs (thermal throttling, a filling cache, a background job).
2. **Multimodality:** two clusters of run times (P-/E-cores, caching, a job starting mid-benchmark), where mean ± σ is misleading.
3. **Outlier-inflated variance:** σ dominated by a few runs.

## Contract
Three new warnings, silenced like the other statistical warnings by
`--suppress-outlier-warnings` (or `--suppress-warnings outliers`):
```
Warning: The run times show a systematic upward trend (+88.7% from the first to the last run, p < 0.001). …
Warning: The run time distribution looks multimodal (bimodality coefficient 0.75 > 0.556, with separated groups of runs). …
Warning: 99% of the variance is caused by 2 outliers (severely inflated). …
```
The JSON gets a per-result `diagnostics` block (fields omitted when not computable):
`trend_rel`, `trend_p`, `bimodality`, `separated_modes`, `outlier_variance_fraction`, `outlier_count`.

All diagnostics use the final run times in **run order**: after `--first-run`,
`--omit-failed-runs` and `--discard-outliers`. They are computed per command, so
they also work with `--schedule round-robin`.

## Methods (`src/stats/diagnostics.rs`)
| Check | Method | Fires when |
|---|---|---|
| Trend | Theil–Sen slope × (n−1) / median; Mann–Kendall with tie correction and continuity correction; series > 1000 runs thinned evenly (O(n²)) | n ≥ 8, \|change\| ≥ 5 % **and** p < 0.001 |
| Multimodality | Sarle's bimodality coefficient (bias-corrected skewness/kurtosis) **and** a Gaussian KDE (normal-reference bandwidth) with ≥ 2 modes, each owning ≥ 10 % of the runs, separated by a valley ≤ ½ of the lower peak (shallow valleys merged first) | n ≥ 30, BC > 5/9 and separated modes |
| Inflated variance | 1 − var(without modified-Z outliers) / var(all) | fraction ≥ 50 % and outliers < 10 % of the runs |

### Deviations from the issue sketch, and why
- **p < 0.001 instead of 0.01:** run times are rarely independent, which makes Mann–Kendall anticonservative. At 0.01, iid noise already gave ~1 % false trend warnings.
- **BC alone is not enough:** skewed unimodal distributions (the usual shape of run times) reach BC > 5/9. Requiring separated KDE modes fixed this. Ashman's D on a 2-means split was tried first and still misfired on exponential tails.
- **Multimodality uses all runs:** with MAD-based Z-scores, a 20 % minority cluster counts entirely as "outliers" and would be removed. A few isolated outliers can't form a mode, because each mode needs 10 % of the runs.
- **Inflated variance only for < 10 % outliers:** larger "outlier" groups are clusters (the multimodal warning). It replaces the generic "Statistical outliers were detected" warning when it applies.

### Measured rates (1000 seeded samples per cell, σ = 5 % noise)
| n | false trend | false multimodal (normal / exponential / lognormal) | detected: 10 % drift | detected: 2 groups 6σ apart, 50/50 · 80/20 |
|---|---|---|---|---|
| 50 | 0.1 % | 0 / 0 / 0 % | 66 % | 28 % · 15 % |
| 200 | 0 % | 0 / 0 / 0 % | 100 % | 100 % · 97 % |
| 1000 | 0 % | 0 / 0 / 0 % | 100 % | 100 % · 100 % |

## Tests
- Unit (`stats::diagnostics`): erfc reference values, ramp up/down, white noise (20 seeds), constant samples, a slow first run alone, thinning of 20k runs, 50/50 and 80/20 clusters, normal/exponential/lognormal (no multimodality), 3 huge outliers, short samples.
- Integration (`tests/diagnostics_tests.rs`): no warnings on stable benchmarks; on unix, real commands for a ramp (+ the JSON block), two alternating groups (multimodal, not "outliers"), rare huge outliers (replaces the generic warning), and suppression.
