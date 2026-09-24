# Specification: Multi-Parameter Scans and Cross-Parameter Matrix

**Issue:** [#45](https://github.com/UnitedOpen-Source/joulex/issues/45)  
**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** In Progress  
**Upstream Reference:** `sharkdp/hyperfine#575`

---

## 1. Overview & Context

Users frequently need to benchmark across multiple independent dimensions:
- Multiple numeric ranges: e.g. matrix size $N \in [10, 50]$ and thread count $T \in [1, 8]$.
- Numeric ranges combined with categorical options: e.g. thread count $T \in [1, 4]$ and compiler flags $O \in \{\text{O0}, \text{O2}, \text{O3}\}$.
- Numeric ranges combined with input files: e.g. batch size $B \in [16, 64]$ and file paths from `urls.txt`.

In upstream `hyperfine`, `-P` was restricted to at most one occurrence and was mutually exclusive with `-L`. This artificial limitation forced users to write fragile shell scripts (such as `$(seq ...)`).

This specification establishes full first-class support for:
1. Multiple `--parameter-scan` (`-P`) flags.
2. Mixing `--parameter-scan` with `--parameter-list` (`-L`) and `--parameter-file` (`-F`).
3. Cartesian product expansion across all defined parameter dimensions.

---

## 2. Technical Architecture & Contracts

### 2.1 CLI Interface (`src/cli.rs`)
- `parameter-scan`:
  - `ArgAction::Append`
  - `num_args(3)`
  - `value_names(["VAR", "MIN", "MAX"])`
- `parameter-list` and `parameter-file`:
  - Remove `.conflicts_with_all(["parameter-scan", "parameter-step-size"])`.
- `parameter-step-size` (`-D`):
  - Retain `.requires("parameter-scan")`.
  - If multiple `-P` arguments are provided alongside `-D`, raise an informative validation error:
    `"The '--parameter-step-size' ('-D') option cannot be used when multiple '--parameter-scan' ('-P') options are specified."`

### 2.2 Parameter Parsing & Cartesian Product (`src/command.rs`)
- Unify parameter definitions into:
  `param_names_and_values: Vec<(&str, Vec<ParameterValue>)>`
- For each `-P <name> <min> <max>`:
  - If single `-P` and `-D <step>` is given, use `<step>`.
  - Parse `<min>`, `<max>`, `<step>` as either `i32` or `Decimal`.
  - Generate values using `RangeStep::new(min, max, step)`.
  - Convert into `Vec<ParameterValue::Numeric>`.
- For each `-L <name> <values>`:
  - Tokenize and convert into `Vec<ParameterValue::Text>`.
- For each `-F <name> <file>`:
  - Read non-empty lines and convert into `Vec<ParameterValue::Text>`.
- Duplicate parameter names across all types (`-P`, `-L`, `-F`) are detected and rejected.
- Generate cartesian product across commands and all parameter dimensions:
  $$\text{Total Benchmarks} = |\text{commands}| \times \prod_{p \in \text{params}} |V(p)|$$

---

## 3. Test Plan

1. **Unit Tests (`src/command.rs`):**
   - Multiple `-P` combinations: `-P a 1 2 -P b 1 2` -> 4 benchmarks.
   - Combined `-P` and `-L`: `-P threads 1 2 -L opt O1,O2` -> 4 benchmarks.
   - Combined `-P` and `-F`: `-P n 1 2 -F file ...` -> cross product.
   - Rejection of `-D` with multiple `-P`.
2. **Execution Order Tests (`tests/execution_order_tests.rs`):**
   - Verify commands are invoked with correctly expanded values for multiple `-P` and `-P` + `-L`.
3. **Integration Tests (`tests/integration_tests.rs`):**
   - Verify CLI execution with `-P a 1 2 -P b 1 2 'echo {a} {b}'`.
   - Verify CSV export properly includes both `parameter_a` and `parameter_b`.
