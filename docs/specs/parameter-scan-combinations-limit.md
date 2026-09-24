# Spec: Parameter Scan Combinations Limit & OOM Protection (Issue #104)

## 1. Problem Statement
When multiple parameter scans (`-P`), lists (`-L`), and files (`-F`) are combined across commands, Joulex eagerly builds every `Command` struct in memory.
While individual `-P` ranges are capped at 100,000 steps via `RangeStep::MAX_PARAMETERS`, the Cartesian product across multiple parameters had no upper bound or overflow protection. For example:
```sh
joulex -N -r 1 -P a 1 20000 -P b 1 20000 'echo {a} {b}'
```
produces 400,000,000 combinations, leading to memory exhaustion (OOM) and massive unresponsiveness.

## 2. Technical Design
1. **Cartesian Product Bound & Overflow Checking:**
   In `Commands::from_cli_arguments` (`src/command.rs`), before allocating memory or expanding combinations:
   - Compute total combinations safely using `try_fold` with `checked_mul`:
     ```rust
     let param_combinations = param_names_and_values
         .iter()
         .try_fold(1usize, |acc, (_, v)| acc.checked_mul(v.len()));
     let total_benchmarks = param_combinations
         .and_then(|p| p.checked_mul(command_strings.len()));
     ```
   - Validate against `max_benchmarks`:
     Default limit is `MAX_BENCHMARKS = 100_000`.
   - If total exceeds the limit or arithmetic overflows:
     Fail immediately with a clear error:
     `The parameter combinations would create more than {max_benchmarks} benchmarks ({breakdown}). Reduce the ranges, use --parameter-step-size, or override with --max-benchmarks.`
2. **CLI Flag `--max-benchmarks <NUM>`:**
   Add `--max-benchmarks` to allow users to intentionally configure the threshold for large benchmark suites.

## 3. Verification Plan
- Unit tests in `src/command.rs`:
  - Assert combination overflow or exceeding limit fails immediately without allocating.
  - Assert `--max-benchmarks` allows overriding the limit when explicitly requested.
- Integration test in `tests/integration_tests.rs`:
  - Run `joulex -N -r 1 -P a 1 20000 -P b 1 20000 'echo {a} {b}'` and verify immediate exit with error message.
  - Run valid multi-parameter scan `-P a 1 100 -P b 1 100` and verify it succeeds.
