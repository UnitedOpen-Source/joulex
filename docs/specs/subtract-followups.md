# Spec: `--subtract` Follow-ups — Baseline Uncertainty Propagation & Energy Subtraction (#202)

## Context & Motivation

In PR #203 (#56), `--subtract CMD` was introduced to measure a baseline command and subtract its mean wall, user, and system time from benchmark runs.
Issue #202 identified two key refinements:

1. **Baseline Uncertainty Propagation**:
   - Each run is computed as $t_{net} = \max(t_{raw} - \bar{t}_{base}, 0)$.
   - The reported $\sigma$ is the run-to-run sample standard deviation of the benchmarked command.
   - However, the subtracted mean has its own estimation uncertainty: $\text{Var}(\bar{t}_{base}) = \frac{\sigma_{base}^2}{n_{base}}$.
   - The net mean $\bar{t}_{net} = \bar{t}_{cmd} - \bar{t}_{base}$ has combined standard error:
     $$\text{SE}_{net} = \sqrt{\frac{\sigma_{cmd}^2}{n_{cmd}} + \frac{\sigma_{base}^2}{n_{base}}}$$
   - This uncertainty must be exported in JSON (`baseline.net_mean_stderr`) and accounted for in `--target-precision` (via Welch–Satterthwaite degrees of freedom) and in `--compare` / `--deep-stats` hypothesis testing.

2. **Energy Subtraction**:
   - While wall, user, and system times were subtracted, energy was not measured for the baseline.
   - When `--energy` is active, the baseline command runs should also be wrapped with the energy sampler.
   - The baseline's mean energy in Joules is recorded in `baseline.energy_joules`.
   - Each run's energy measurement is subtracted: $e_{net} = \max(e_{raw} - \bar{e}_{base}, 0)$.
   - Per-run energy vector, `mean_energy_joules`, and `mean_watts` reflect net energy consumption.

---

## Technical Contract

### 1. Data Contract (`src/benchmark/benchmark_result.rs`)

`Baseline` struct extended with:
```rust
pub struct Baseline {
    pub command: String,
    pub mean: Second,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stddev: Option<Second>,
    pub user: Second,
    pub system: Second,
    pub runs: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub clamped_runs: usize,
    /// Combined standard error of the net mean:
    /// sqrt(cmd_stderr^2 + base_stderr^2) = sqrt(var_cmd / n_cmd + var_base / n_base)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_mean_stderr: Option<Second>,
    /// Mean energy consumed by the baseline in Joules (when --energy is active)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_joules: Option<f64>,
}
```

### 2. Statistical Computations

#### Combined Standard Error (`net_mean_stderr`)
- For $n_{cmd} \ge 2$:
  - $\text{SE}_{cmd}^2 = \frac{\sigma_{cmd}^2}{n_{cmd}}$ (or $0.0$ if all runs are identical).
  - $\text{SE}_{base}^2 = \frac{\sigma_{base}^2}{n_{base}}$ if $\sigma_{base}$ is present, else $0.0$.
  - $\text{net\_mean\_stderr} = \sqrt{\text{SE}_{cmd}^2 + \text{SE}_{base}^2}$.
- For $n_{cmd} < 2$: `net_mean_stderr = None`.

#### Target Precision (`src/stats/precision.rs`)
- `relative_ci_half_width_with_baseline(xs, baseline_stddev, baseline_runs)`:
  - Computes $\text{SE}_{net} = \sqrt{\text{SE}_{cmd}^2 + \text{SE}_{base}^2}$.
  - Computes effective degrees of freedom $\nu$ via Welch–Satterthwaite:
    $$\nu = \frac{(\text{SE}_{cmd}^2 + \text{SE}_{base}^2)^2}{\frac{(\text{SE}_{cmd}^2)^2}{n_{cmd} - 1} + \frac{(\text{SE}_{base}^2)^2}{n_{base} - 1}}$$
  - Returns $t_{0.975}(\nu) \times \frac{\text{SE}_{net}}{\bar{t}_{net}}$.
  - If $\text{SE}_{net} = 0$, returns `Some(0.0)`.

#### Two-Sample Bootstrap Comparison (`src/stats/deep.rs`, `src/compare.rs`)
- `welch_t_with_variance(a, b, var_base_a, var_base_b)`:
  $$t = \frac{\bar{X}_a - \bar{X}_b}{\sqrt{\frac{s_a^2}{n_a} + \text{var}_{base,a} + \frac{s_b^2}{n_b} + \text{var}_{base,b}}}$$
- `compare_samples_with_baseline_variance(a, b, var_base_a, var_base_b)` incorporates the baseline variances both in observed $t$ and during null distribution bootstrap resampling.

### 3. Energy Subtraction Pipeline

1. **Baseline Measurement (`measure_baseline`)**:
   - If `options.measure_energy`: start and stop `energy_sampler` around warmup and benchmark runs.
   - Compute `baseline_energy = mean(valid_energy_samples)`.
   - Store in `baseline.energy_joules`.
   - If `output_style != Disabled`, print `Energy (mean): X.XXX J`.

2. **Benchmark Execution (`BenchmarkRunner`)**:
   - For every timing iteration (including initial run):
     ```rust
     let net_energy = match (raw_energy, self.baseline.as_ref().and_then(|b| b.energy_joules)) {
         (Some(e), Some(base_e)) => Some((e - base_e).max(0.0)),
         (e, _) => e,
     };
     ```
   - Store `net_energy` in `self.energy_measurements`.
   - `mean_energy_joules` and `energy_joules` in `BenchmarkResult` automatically reflect net energy.
   - `mean_watts` evaluates net power: $\frac{\text{mean\_energy\_joules}}{\text{mean\_time}}$.

---

## Verification & Testing Plan

1. **Unit Tests**:
   - `stats::precision`: test `relative_ci_half_width_with_baseline` with zero baseline variance, non-zero baseline variance, constant samples, and degrees of freedom scaling.
   - `stats::deep`: test `compare_samples_with_baseline_variance` showing increased p-value when baseline uncertainty is introduced.
   - `benchmark::mod`: unit tests with `MockExecutor` for energy subtraction clamping to 0, and `net_mean_stderr` generation in `BenchmarkResult`.

2. **Integration Tests (`tests/subtract_tests.rs`)**:
   - Baseline JSON export contains `baseline.net_mean_stderr`.
   - Runs with `--target-precision` and `--subtract`.
   - Comparisons with `--compare` where baseline has subtracted offset.
   - CLI help text reflects energy subtraction.
