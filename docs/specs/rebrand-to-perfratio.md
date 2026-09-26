# Specification: Rebrand Project from Joulex to Perfratio

**Issue / Ref:** Rebrand to `perfratio`  
**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** In Progress  

---

## 1. Overview & Motivation

The project is evolving from its origins as a performance-per-watt fork of Hyperfine into **`perfratio`**: a multi-dimensional benchmarking and efficiency ratio analysis tool.

### Why `perfratio`?
Performance engineering is fundamentally about **ratios**:
1. **Speedup Ratio:** $\frac{\text{baseline runtime}}{\text{optimized runtime}}$ accompanied by bootstrapped 95% confidence intervals and hypothesis testing ($p$-values).
2. **Hardware Counter Efficiency Ratios:**
   - Instructions Per Cycle (IPC) & Cycles Per Instruction (CPI).
   - Cache Miss Rate ($\frac{\text{cache misses}}{\text{cache references}} \times 100\%$).
   - Branch Misprediction Rate ($\frac{\text{branch misses}}{\text{branches}} \times 100\%$).
   - Vectorization Ratio ($\frac{\text{SIMD instructions}}{\text{total instructions}} \times 100\%$).
3. **Energy Efficiency & Performance per Watt:**
   - Joules per operation and Energy-Delay Product ($\text{Joules} \times \text{Seconds}$).
   - Active Power in Watts.
4. **Statistical Ratios:**
   - Outlier percentage via Tukey's Fences (IQR).
   - PMU counter multiplexing active time ratio ($\frac{\text{time\_running}}{\text{time\_enabled}} \times 100\%$).

---

## 2. Technical Architecture & Modifications

### 2.1 Package Metadata (`Cargo.toml`, `CITATION.cff`)
- `Cargo.toml`:
  - `name = "perfratio"`
  - `description = "Multi-dimensional command-line benchmarking tool with Performance per Watt, Hardware Counter Ratios, and Deep Statistics"`
  - `include`: update `/doc/joulex.1` to `/doc/perfratio.1`.
  - `[package.metadata.binstall]`: update binary and archive paths to `perfratio`.
- `CITATION.cff`:
  - `title: perfratio`

### 2.2 CLI & Environment Variables
- `src/cli.rs`:
  - Rename CLI command name to `"perfratio"`.
  - Update user-facing strings, examples, and fish completions to `perfratio`.
- `src/benchmark/executor.rs`:
  - Set `$PERFRATIO_ITERATION` in addition to `$JOULEX_ITERATION` and `$HYPERFINE_ITERATION` for full backwards compatibility.
- `build.rs`:
  - Generate shell completions for `perfratio`.
- `src/main.rs`:
  - Generate completions under `perfratio` binary name.

### 2.3 System Check & Exports
- `src/system_check.rs`:
  - Support `PERFRATIO_SYSTEM_CHECK_ROOT` (fallback to `JOULEX_SYSTEM_CHECK_ROOT`).
- `src/export/html.rs`:
  - Update report title to `<title>perfratio report</title>`.
- `src/export/mod.rs`:
  - Temporary export file prefix changed to `.perfratio-tmp-`.
- `src/output/progress_bar.rs`:
  - ETA key registered as `perfratio_eta`.

### 2.4 Documentation & Manpage
- `README.md`:
  - Rebrand title to `# perfratio`.
  - Update command examples to `perfratio <command>...`.
  - Document heritage (formerly joulex, forked from hyperfine and inspired by bench/Criterion).
- `doc/perfratio.1`:
  - Create `doc/perfratio.1` (and retain `doc/joulex.1` if needed for backwards compatibility).

### 2.5 Integration Tests
- Update `assert_cmd::cargo::cargo_bin!("perfratio")` in `tests/common.rs` and `tests/integration_tests.rs`.

---

## 3. Test Plan
- Run `cargo check` and `cargo test` ensuring 100% of integration tests pass.
- Verify CLI completions build and generate scripts with `perfratio`.
- Verify `$PERFRATIO_ITERATION` environment variable forwarding.
