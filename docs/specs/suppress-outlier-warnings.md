# Specification: Suppress Statistical Outlier Warnings

**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** Approved  
**Issue:** [UnitedOpen-Source/joulex#13](https://github.com/UnitedOpen-Source/joulex/issues/13)  
**Upstream Reference:** `sharkdp/hyperfine#528`

---

## 1. Overview
In CI pipelines, virtualized environments (e.g. Docker, WSL2, GitHub Actions), and automated scripts, slight scheduling jitter often triggers the yellow `Warning: Statistical outliers were detected` message. This warning clutters log files and automated reports.

## 2. Interface Contract
- Flag: `--suppress-outlier-warnings` (`ArgAction::SetTrue`).
- When enabled, `joulex` suppresses `Warnings::SlowInitialRun` and `Warnings::OutliersDetected`.
- Other non-outlier warnings (such as `NonZeroExitCode`) continue to be reported normally.

## 3. Verification Plan
- Unit test ensuring `suppress_outlier_warnings` flag in `Options` is correctly parsed.
- Integration test running commands known to produce outliers with and without `--suppress-outlier-warnings`, asserting that the warning is absent when the flag is supplied.
