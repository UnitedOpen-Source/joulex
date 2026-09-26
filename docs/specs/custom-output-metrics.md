# Spec: Custom Output Metrics (`--output-metric 'NAME=REGEX'`) (#90)

## Problem / Motivation
References: `sharkdp/hyperfine#170`, `#370`, `#718`, `#90`.

Many command-line utilities, benchmarks, compilers, and test suites output domain-specific numeric metrics during execution (e.g. database query latency, transactions per second, compilation phase timing, iteration counts, memory allocated).

Currently, users can only benchmark wall-clock time (and OS rusage/energy). To measure domain metrics across multiple runs with statistical rigor (mean, stddev, median, min, max, percentiles), users must write external parsing scripts.

## Contract

```
--output-metric <NAME=REGEX>   Extract a custom numeric metric from the command's stdout.
                               NAME must be alphanumeric/underscores.
                               REGEX must contain at least one capture group.
                               The metric value is parsed as a float from the first
                               capture group of the last match in the output.
                               Can be specified multiple times for different metrics.
```

### 1. CLI Validation & Format
- Argument syntax: `<NAME>=<REGEX>`.
  - `NAME`: non-empty, ASCII alphanumeric and underscores only (`[a-zA-Z0-9_]+`).
  - `REGEX`: valid regular expression containing at least one capturing group (`captures_len() >= 2`).
- Invalid name, missing `=`, invalid regex, or missing capture group causes `OptionsError::InvalidOutputMetric(String)` with an actionable explanation.
- Conflicts with `--show-output` (`-d`) because stdout cannot be both displayed in real time and captured safely.

### 2. Execution & Extraction
- When `--output-metric` is specified, the benchmark runner captures stdout (bounded up to 1 MiB tail buffer to protect against unbounded memory growth).
- For each metric, the regular expression is executed against the captured output:
  - If multiple matches occur, the **last** match in the stream is selected.
  - The value of capture group 1 is parsed as an `f64`.
  - If no match occurs, or the capture cannot be parsed as a float:
    - The run fails with error: `Metric '<NAME>' could not be extracted from output in <when>`.
    - If `-i` / `--ignore-failure` is active, the failure is ignored according to standard failure handling policies.

### 3. Statistics & Reporting
- For each metric across successful runs:
  - `mean`: arithmetic mean.
  - `stddev`: sample standard deviation (when runs > 1).
  - `median`: 50th percentile.
  - `min`, `max`: extreme values.
- Terminal Output:
  Printed underneath `Time (mean ± σ)`:
  ```
    <name> (mean ± σ):    <mean> ± <stddev>
  ```
- JSON Export:
  Each benchmark entry in `"results"` includes:
  `"custom_metrics": { "<name>": [<val1>, <val2>, ...] }`
- CSV Export:
  For each metric, columns are appended:
  `mean_<name>`, `stddev_<name>`, `median_<name>`, `min_<name>`, `max_<name>`.

## Verification Plan
- Unit tests:
  - `OutputMetric::from_str`: valid formats, missing `=`, bad names, missing capture groups, bad regex.
  - `OutputMetric::extract`: single match, multiple matches (selects last), float formats (integer, decimal, scientific notation), non-numeric capture, no match.
- Integration tests:
  - Single `--output-metric` with command printing metric.
  - Multiple `--output-metric` flags extracting independent metrics.
  - Missing metric causing command failure.
  - Missing metric ignored with `-i`.
  - JSON export contains `"custom_metrics"`.
  - CSV export contains `mean_<name>` columns.
  - Conflicts with `--show-output`.
