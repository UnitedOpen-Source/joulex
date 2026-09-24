# Specification: Relative Speed Table Annotations with --sort=command and --reference

**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** In Progress  
**Issue:** [UnitedOpen-Source/joulex#19](https://github.com/UnitedOpen-Source/joulex/issues/19)  
**Upstream References:** `sharkdp/hyperfine#811`, `sharkdp/hyperfine#879`

---

## 1. Problem Statement
When running benchmarks with `--sort=command` alongside `--reference`:
The `Relative speed comparison` table displays numerical ratios (e.g. `1.50 ± 0.02`) without indicating directionality. Since ratios are normalized against the reference, users cannot immediately discern whether `1.50` denotes 1.50x faster or 1.50x slower than the reference command.

## 2. Technical Design
In `Scheduler::print_relative_speed_comparison` (`src/benchmark/scheduler.rs`):
When `sort_order_speed_comparison == SortOrder::Command` and `reference_command` is specified:
1. Identify the reference command name from `annotated_results.iter().find(|r| r.is_reference)`.
2. For each non-reference item:
   - If `item.relative_ordering == Ordering::Less`: append `times faster than {reference_name}`.
   - If `item.relative_ordering == Ordering::Greater`: append `times slower than {reference_name}`.
   - If `item.relative_ordering == Ordering::Equal`: append `as fast as {reference_name}`.
3. Reference command row remains at `1.00` without suffix.

## 3. Verification Plan
- Integration test in `tests/integration_tests.rs`:
  - Run `--sort=command --reference="sleep 2.0" "sleep 1.0" "sleep 3.0"`.
  - Assert stdout contains:
    - `Relative speed comparison`
    - `times faster than sleep 2.0`
    - `times slower than sleep 2.0`
- Existing snapshot tests in `src/benchmark/scheduler.rs` continue to pass without regression.
