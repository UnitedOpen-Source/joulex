# Specification: Import JSON Benchmarks

**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** Approved  
**Issue:** [UnitedOpen-Source/joulex#6](https://github.com/UnitedOpen-Source/joulex/issues/6)  
**Upstream References:** `sharkdp/hyperfine#607`, PR `sharkdp/hyperfine#873`, `sharkdp/hyperfine#830`

---

## 1. Executive Summary

Users frequently benchmark before and after applying changes (system tuning, hardware upgrades, compiler flags, or pull requests) across different sessions or CI runs.

Currently, comparing benchmarks requires running all commands simultaneously in the same process. `--import-json <FILE>` allows loading previously exported JSON benchmarks into `joulex` to:
1. Participate in relative speed comparisons against live benchmarks without re-executing the baseline.
2. Directly convert previous benchmark runs into other formats (Markdown, CSV, AsciiDoc, Org-mode) without requiring dummy commands.

## 2. Interface Contract

### 2.1 CLI Flags
- `--import-json <FILE>`: Repeatable argument (`ArgAction::Append`).
- Makes `command` positional argument optional (`required_unless_present_any(["generate-completions", "import-json"])`).

### 2.2 Behavior
- Deserializes `HyperfineSummary` JSON containing `Vec<BenchmarkResult>`.
- Preserves all time, memory, exit codes, and energy metrics.
- Backfills `command_with_unused_parameters` with `command` if missing from on-disk schema.
- Displays imported benchmarks in console output as `Benchmark N: <cmd> (imported)`.
- Re-indexes subsequent live benchmarks so display numbers follow seamlessly.
- Allows invocations with zero live commands (pure conversion/comparison mode).

## 3. Test & Verification Plan
1. **Unit tests in `src/import.rs`**:
   - JSON serialization and deserialization round-trip.
   - Graceful fallback for missing optional fields.
   - Non-existent file error handling.
2. **Integration tests in `tests/integration_tests.rs`**:
   - Live benchmark + imported benchmark comparison.
   - Pure format conversion without live commands (`--import-json ... --export-markdown ...`).
   - Multiple `--import-json` files combined.
