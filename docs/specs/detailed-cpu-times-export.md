# Specification: Detailed User and System CPU Times Export

**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** Approved  
**Issue:** [UnitedOpen-Source/joulex#11](https://github.com/UnitedOpen-Source/joulex/issues/11)  
**Upstream Reference:** `sharkdp/hyperfine#660`

---

## 1. Overview
Currently, `joulex` exports all wall-clock timing samples in `times: Vec<Second>`, but only exports the aggregate means for user and system CPU times (`user: Second`, `system: Second`).

External tools (such as Jupyter notebooks, CI benchmarking bots, and statistical analyzers) need access to individual CPU run samples to compute quantiles, medians, standard deviations, and generate distribution plots.

## 2. Specification & Contracts
- Extend `BenchmarkResult` with:
  ```rust
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub user_times: Option<Vec<Second>>,

  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub system_times: Option<Vec<Second>>,
  ```
- In `Benchmark::run`: Populate `user_times: Some(times_user)` and `system_times: Some(times_system)`.
- Non-breaking backwards-compatible JSON schema extension: old JSON files without these fields continue to deserialize with `None`.

## 3. Verification Plan
- Unit tests validating serialization and deserialization of `user_times` and `system_times`.
- Integration tests ensuring `results.json` contains `user_times` and `system_times` arrays with matching sample lengths.
