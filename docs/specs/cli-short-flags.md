# Specification: Short Flags for Style, Sort, Show-Output, Output, and Input

**Issue:** [#78](https://github.com/UnitedOpen-Source/joulex/issues/78)  
**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** Completed  
**Upstream Reference:** `sharkdp/hyperfine#689`, `sharkdp/hyperfine#690`

---

## 1. Overview & Context

Command-line benchmarking often happens interactively in developer terminals where typing long arguments slows down exploratory benchmarking sessions.

Currently, frequently used options require full flags:
- `--style <TYPE>`
- `--sort <METHOD>`
- `--show-output`
- `--output <WHERE>`
- `--input <WHERE>`

This feature adds ergonomic short flag counterparts:
- `-l, --style <TYPE>`
- `-t, --sort <METHOD>`
- `-d, --show-output`
- `-O, --output <WHERE>`
- `-I, --input <WHERE>`

All existing short flags in `joulex` (`w`, `m`, `M`, `r`, `s`, `p`, `C`, `c`, `P`, `D`, `L`, `F`, `S`, `N`, `i`, `E`, `u`, `e`, `n`, `h`, `V`) are preserved with zero conflicts.

---

## 2. Technical Architecture & Contracts

### 2.1 CLI Interface (`src/cli.rs`)
Add `.short(...)` to the arguments:
- `style`: `.short('l')`
- `sort`: `.short('t')`
- `show-output`: `.short('d')`
- `output`: `.short('O')`
- `input`: `.short('I')`

### 2.2 Documentation & Manpages
- Update `doc/joulex.1` and `doc/hyperfine.1`:
  - `\fB\-l\fR, \fB\-\-style\fR \fITYPE\fP`
  - `\fB\-t\fR, \fB\-\-sort\fR \fIMETHOD\fP`
  - `\fB\-d\fR, \fB\-\-show\-output\fR`
  - `\fB\-O\fR, \fB\-\-output\fR \fIWHERE\fP`
  - `\fB\-I\fR, \fB\-\-input\fR \fIWHERE\fP`
- Completions in `build.rs` will automatically include the new short options.

---

## 3. Test Plan

### 3.1 Integration Tests (`tests/integration_tests.rs`)
- Add integration tests verifying:
  - `-l basic` parses and functions as expected.
  - `-t command` parses and functions as expected.
  - `-d` shows output.
  - `-O null` / `-O <file>` redirects output.
  - `-I <file>` feeds input.
- Verify generated completions contain `-l`, `-t`, `-d`, `-O`, `-I`.
