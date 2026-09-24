# Specification: Round-Robin / Interleaved Benchmark Execution Schedule

**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** In Progress  
**Issue:** [UnitedOpen-Source/joulex#12](https://github.com/UnitedOpen-Source/joulex/issues/12)  
**Upstream Reference:** `sharkdp/hyperfine#822`

---

## 1. Problem Statement
In existing command-line benchmarking tools, multiple commands are executed in **grouped** order: all warmup and timing runs for Command A are completed before Command B begins.

When benchmarking commands on modern CPUs with dynamic voltage and frequency scaling (DVFS), thermal throttling, or varying background workloads:
1. The earlier command runs under cooler hardware conditions at peak boost clocks.
2. Subsequent commands run under thermally constrained conditions or throttling.
3. Temporary spikes in background system activity distort only the command executing during the spike rather than affecting all candidates equally.

## 2. Solution Design
Introduce an interleaved **round-robin** execution schedule mode.

### 2.1 CLI Interface
- `--schedule <MODE>`: Select benchmark execution schedule.
  - `grouped` (default): Run all iterations of command 1, then command 2, etc.
  - `round-robin`: Run 1 iteration of command 1, 1 iteration of command 2, repeating until all iterations are completed.
  - Aliases: `sequential`, `interleaved` map to `round-robin`.
- `--round-robin`: Shortcut boolean flag equivalent to `--schedule round-robin`.

### 2.2 Lifecycle & Execution Ordering
For $N$ commands (including reference command if present):

1. **Setup Phase:**
   Run `--setup` once for each command before any warmup or measurement iterations begin.

2. **Warmup Phase (if `--warmup W` is specified, $W > 0$):**
   For $w \in 0..W$:
     For $c \in 0..N$:
       Run `prepare(c, Warmup(w))`
       Run `warmup(c, Warmup(w))`
       Run `conclude(c, Warmup(w))`

3. **Initial Measurement & Run Count Determination:**
   For $c \in 0..N$:
     Run `prepare(c, Benchmark(0))`
     Run `measurement(c, Benchmark(0))`
     Run `conclude(c, Benchmark(0))`
     Determine required run count $K_c$ based on execution time and `--runs` / `--min-runs` / `--max-runs` bounds.
   Let $K_{\max} = \max(K_0, K_1, \dots, K_{N-1})$.

4. **Interleaved Timing Iterations:**
   For iteration $i \in 1..K_{\max}$:
     For $c \in 0..N$:
       If $i < K_c$:
         Run `prepare(c, Benchmark(i))`
         Run `measurement(c, Benchmark(i))`
         Run `conclude(c, Benchmark(i))`

5. **Cleanup Phase:**
   Run `--cleanup` once for each command after all its iterations are complete.

6. **Results & Reporting:**
   Compute statistical summaries (mean, stddev, median, min, max, energy, CPU times) for each benchmark and render results and relative comparisons according to the chosen `--style`.

## 3. Verification Plan
- Unit tests for CLI parsing and `ScheduleMode` in `src/options.rs`.
- Execution order test in `tests/execution_order_tests.rs` asserting exact interleaved output order:
  `cmd 1`, `cmd 2`, `cmd 1`, `cmd 2`.
- Combined execution order test asserting `setup`, `prepare`, `conclude`, and `cleanup` sequences under `--schedule round-robin`.
- Snapshot test in `src/benchmark/scheduler.rs`.
