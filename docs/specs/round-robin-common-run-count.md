# Spec: Common Run Count Across Runners in Round-Robin Scheduling (Issue #102, #111)

## 1. Problem Statement
1. **Unequal Run Count Tail (Issue #102):**
   In `--schedule round-robin` mode, each `BenchmarkRunner` computes its iteration `count` independently during the initial measurement:
   `runs_in_min_time = min_benchmarking_time / t_initial`.
   When benchmarking commands with different execution durations (e.g., Command A takes 5ms and Command B takes 200ms), Command A calculates ~600 runs while Command B calculates ~15 runs.
   During execution, once B finishes its 15 runs, A continues running alone for the remaining 585 iterations without interleaving.
   This non-interleaved tail defeats the purpose of round-robin scheduling (mitigating temporal bias, CPU thermal throttling, and DVFS frequency scaling drift).

2. **Misleading Schedule Alias (Issue #111):**
   `--schedule` accepted `grouped | round-robin | sequential | interleaved` and mapped `sequential` to `RoundRobin`.
   Most users interpret "sequential" as running all iterations of one command after another (which is the default grouped schedule), making `sequential` an ambiguous and misleading alias for round-robin.

## 2. Technical Design
1. **Common Run Count Calculation (`src/benchmark/scheduler.rs`):**
   In round-robin mode, after all runners complete their initial measurement:
   - Calculate the total duration of a single round:
     `per_round = sum of initial_total_time across all runners`.
   - Calculate common iteration count `common`:
     - If explicit `--runs N` (`min == max`): `common = min`.
     - Otherwise:
       `n = (self.options.min_benchmarking_time / per_round) as u64;`
       `n = n.max(self.options.run_bounds.min);`
       `common = self.options.run_bounds.max.map_or(n, |m| n.min(m)).max(1);`
   - Set `runner.count = common` for all runners.
   - All runners will execute exactly the same number of iterations, interleaved from the first run to the last.

2. **Track Initial Total Time on `BenchmarkRunner` (`src/benchmark/mod.rs`):**
   - Add `pub initial_total_time: f64` to `BenchmarkRunner`.
   - Record `res.time_real + self.executor.time_overhead() + preparation_overhead + conclusion_overhead` in `run_initial_measurement()`.

3. **CLI & Schedule Mode Cleanup (`src/cli.rs`, `src/options.rs`, `doc/joulex.1`):**
   - In `src/cli.rs`, remove `sequential` from `--schedule`'s `value_parser`, keeping `["grouped", "round-robin", "interleaved"]`.
   - Clarify `--help` text in `src/cli.rs` and documentation in `doc/joulex.1`.
   - In `src/options.rs`, update parser to handle `round-robin` and `interleaved`.
   - Update tests to verify `interleaved` maps to `RoundRobin`.

## 3. Verification Plan
- Integration test in `tests/integration_tests.rs`:
  - Run `--schedule round-robin` with a fast command (`sleep 0.005`) and a slow command (`sleep 0.1` or `sleep 0.05`).
  - Export to JSON and verify that both commands have identical `times.len()`.
- Test `--schedule sequential` is rejected by clap.
- Test `--schedule interleaved` correctly maps to `RoundRobin`.
- Run full test suite (`cargo test`), format (`cargo fmt --check`), and clippy (`cargo clippy --all-targets -- -D warnings`).
