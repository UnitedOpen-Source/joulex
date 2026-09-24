# Specification: Parameter File Streaming

**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** Approved  
**Issue:** [UnitedOpen-Source/joulex#5](https://github.com/UnitedOpen-Source/joulex/issues/5)  
**Upstream References:** `sharkdp/hyperfine#813`, PR `sharkdp/hyperfine#908`, PR `sharkdp/hyperfine#910`

---

## 1. Executive Summary

When benchmarking systems over large input dimensions (such as thousands of URLs, test vectors, or files), specifying parameters via `--parameter-list <VAR> <VAL1>,<VAL2>...` is constrained by OS command line length limits (`ARG_MAX`) and quoting complexity.

This specification defines the architecture, CLI interface, and error handling for `--parameter-file <VAR> <FILE>` in `joulex`.

## 2. Interface Contract

### 2.1 CLI Flags
- Flag: `-F, --parameter-file <VAR> <FILE>`
- Action: `ArgAction::Append`
- Arity: 2 arguments (`num_args(2)`)
- Mutually exclusive with `--parameter-scan` (conflicts_with).
- Fully composable with multiple `--parameter-file` instances and with `--parameter-list`.

### 2.2 Behavior
- Reads values line-by-line from `FILE`.
- Supports both LF (`\n`) and CRLF (`\r\n`) line endings.
- Strips trailing blank lines.
- Rejects non-existent files with `Could not read parameter file '<FILE>'`.
- Rejects empty files with `Parameter file '<FILE>' contains no values`.
- Checks duplicate parameter names across all `--parameter-file` and `--parameter-list` declarations.
- Computes cartesian product across all parameter dimensions identically to `--parameter-list`.

## 3. Test & Verification Plan
1. **Unit tests in `src/command.rs`**:
   - Multiple parameter files combined.
   - Mix of `--parameter-file` and `--parameter-list`.
   - Handling of CRLF and trailing empty lines.
2. **Integration tests in `tests/integration_tests.rs`**:
   - Single `--parameter-file` execution.
   - Non-existent file error reporting.
   - Empty file error reporting.
