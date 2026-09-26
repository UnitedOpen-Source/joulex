# Spec: `--metric` Primary Optimization Target (#48)

## Problem / Motivation
References: `sharkdp/hyperfine#736`, `perfratio#48` (`joulex#48`).

By default, benchmarking tools compute all comparisons, relative speed ratios, rankings, summaries, and outlier detection solely from **wall-clock time**. However, modern performance engineering frequently targets other critical dimensions:
1. **Energy consumption (Joules):** Finding the greenest or most energy-efficient implementation (perfratio's core value proposition).
2. **CPU time (`user + system`):** Measuring algorithmic efficiency or multi-process computational work independent of I/O latency or scheduling delays.
3. **Memory footprint (Peak RSS):** Tracking memory regressions, comparing memory-efficient data structures, or optimizing for constrained environments.
4. **User / System time:** Isolating user-space computation vs kernel overhead.

## Proposed Contract

### 1. CLI Option
```
--metric <METRIC>
    Primary metric used for statistics, outlier detection, comparison, and sorting.
    [default: wall]
    [possible values: wall, cpu, user, system, energy, memory]
```

- When `--metric energy` is passed, energy measurement is automatically enabled (`measure_energy = true`). If energy sampling is unsupported or unprivileged on the host system, perfratio fails immediately with an explanatory error.
- Case-insensitive parsing and common aliases (e.g. `wall-clock`, `rss`).

### 2. Metric Abstraction
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    #[default]
    Wall,
    Cpu,
    User,
    System,
    Energy,
    Memory,
}
```

Methods on `Metric`:
- `name(&self) -> &'static str`: e.g. `"wall"`, `"cpu"`, `"energy"`, `"memory"`.
- `display_header(&self) -> &'static str`: `"Time"`, `"CPU Time"`, `"User Time"`, `"System Time"`, `"Energy"`, `"Memory"`.
- `comparison_words(&self) -> (&'static str, &'static str)`:
  - Time variants: `("faster", "slower")`
  - Energy: `("less energy", "more energy")`
  - Memory: `("less memory", "more memory")`
- `verb(&self) -> &'static str`:
  - Time: `"ran"`
  - Energy & Memory: `"used"`

### 3. Primary Samples Extraction
In `BenchmarkResult`:
```rust
pub fn primary_samples(&self, metric: Metric) -> Option<Vec<f64>>
```
- Extracts the appropriate float values:
  - `Wall`: `self.times.clone()`
  - `User`: `self.user_times.clone()`
  - `System`: `self.system_times.clone()`
  - `Cpu`: zip of user and system times, summed per run
  - `Energy`: `self.energy_joules.clone()`
  - `Memory`: `self.memory_usage_byte` cast to `f64`

### 4. Terminal Report & Summary
- The primary statistic header in the terminal matches the chosen metric:
  - If `Wall`: standard `Time (mean ± σ): ...`
  - If `Energy`: `Energy (mean ± σ): 1.250 J ± 0.050 J`, with `Time` on the secondary line.
  - If `Memory`: `Memory (mean ± σ): 12.5 MB ± 0.5 MB`, with `Time` on the secondary line.
  - If `Cpu`: `CPU Time (mean ± σ): ...`, with `Time` on the secondary line.
- The summary table formats comparative ratios using metric-specific wording:
  - Time: `'command A' ran 1.50 ± 0.02 times faster than 'command B'`
  - Energy: `'command A' used 1.50 ± 0.02 times less energy than 'command B'`
  - Memory: `'command A' used 1.50 ± 0.02 times less memory than 'command B'`

### 5. Sorting & Exports
- `SortOrder::MeanTime` (default) sorts by the primary metric's mean value.
- JSON Export:
  - Includes `"metric": "<metric>"` at the top level of the JSON document.
  - Keeps existing `mean`, `stddev`, `times` (wall time) intact for backwards compatibility.
  - Adds `"primary_metric": { "metric": "...", "mean": ..., "stddev": ..., "samples": [...] }` if metric is not `wall`.

## Verification Plan
- Unit tests:
  - CLI argument parsing for `--metric` with all valid variants and aliases.
  - `primary_samples` extraction for every `Metric` variant.
  - Comparison wording and formatting helpers.
- Integration tests:
  - `--metric cpu` on `sleep 0.1` vs a CPU-heavy command inverts relative ranking compared to `--metric wall`.
  - `--metric memory` verifies memory-based summary comparison.
  - `--metric energy` requires energy availability or exits with error code 1.
  - JSON export contains the top-level `"metric"` attribute.
