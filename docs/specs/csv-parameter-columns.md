# Specification: Consistent CSV Parameter Columns

**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** In Progress  
**Issue:** [UnitedOpen-Source/joulex#17](https://github.com/UnitedOpen-Source/joulex/issues/17)  
**Upstream References:** `sharkdp/hyperfine#852`, `sharkdp/hyperfine#902`

---

## 1. Problem Statement
When exporting benchmark results containing parameterized commands alongside unparameterized commands (e.g., using `--reference`) or commands with differing parameter sets to CSV:
1. `CsvExporter` previously extracted parameter header names only from the first command with non-empty parameters.
2. If the reference command was intermediate-exported first, headers lacked parameter columns.
3. If later commands defined parameters not present in the first parameterized command, row lengths differed from header lengths, triggering CSV serialization errors.
4. If commands defined different parameter sets, values were written sequentially rather than keyed to their corresponding column headers.

## 2. Technical Design
1. **Sorted Parameter Key Union:**
   Collect all parameter names across all `BenchmarkResult` items in a `BTreeSet<&str>`, ensuring deterministic alphabetical ordering:
   ```rust
   let mut all_param_names = BTreeSet::new();
   for res in results {
       for param_name in res.parameters.keys() {
           all_param_names.insert(param_name.as_str());
       }
   }
   ```

2. **Unified Header Construction:**
   Append `parameter_{param_name}` to CSV headers for each key in `all_param_names`.

3. **Keyed Row Serialization:**
   For each benchmark result, write the standard timing fields (`command`, `mean`, `stddev`, `median`, `user`, `system`, `min`, `max`), then look up each parameter in `all_param_names`:
   - If present: write `sanitize_csv_value(val)`.
   - If missing: write an empty field `""`.

4. **Guarantees:**
   - Every row has the exact same column count: $8 + |\text{all\_param\_names}|$.
   - Reference commands and non-parameterized commands have empty strings in parameter columns.
   - Heterogeneous parameter keys align with their respective columns.

## 3. Verification Plan
- Unit test in `src/export/csv.rs` verifying CSV output with a reference command (no parameters) and parameterized commands.
- Unit test verifying heterogeneous parameter sets across commands (e.g. command 1 with `param_a`, command 2 with `param_b`).
- Integration test running `joulex --reference 'sleep 0.01' --parameter-list secs 0.01,0.02 'sleep {secs}' --export-csv <FILE>` ensuring valid CSV generation without record length errors.
