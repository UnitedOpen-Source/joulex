# Specification: Rebrand Leftovers & Fix Metadata / Manpage

**Issue:** [#86](https://github.com/UnitedOpen-Source/joulex/issues/86)  
**Author:** Matheus Breguêz <matbrgz@gmail.com>  
**Status:** Completed  
**Upstream Reference:** `sharkdp/hyperfine#903`, `sharkdp/hyperfine#870`, `sharkdp/hyperfine#838`, `sharkdp/hyperfine#907`

---

## 1. Overview & Context

Following the fork from `hyperfine` to `joulex` under the `UnitedOpen-Source` organization, several metadata files, user-facing error/warning strings, and documentation files retained broken URLs or outdated references:

1. **Cargo.toml:** Repository and homepage URLs had casing `unitedopensource` instead of canonical `UnitedOpen-Source`, returning HTTP 404.
2. **README.md:** CI workflow badge pointed to `unitedopensource/joulex`. MSRV stated Rust 1.76 instead of `rust-version = "1.88.0"`. Installation instructions cited `hyperfine` instead of `joulex`.
3. **CITATION.cff:** Cff metadata still described `hyperfine` v1.16.1.
4. **CI/CD Workflow (`.github/workflows/CICD.yml`):** Packaging scripts expect `doc/${{ crate_name }}.1` (i.e. `doc/joulex.1`). Currently only `doc/hyperfine.1` exists.
5. **Warning & Help text:** `src/output/warnings.rs` warned that "hyperfine can not calibrate the shell startup time". `src/cli.rs` referenced `hyperfine will show a ...`.
6. **Manpage formatting & modern flags:** Manpage contained font escape typos (`\fi` instead of `\fI`), unescaped hyphen (`time-unit`), lacked `-C` shorthand for `--conclude`, lacked `--reference` and `--reference-name`, and lacked joulex-specific features (`--energy`, `--deep-stats`, `--schedule`, `--import-json`, etc.).

---

## 2. Technical Architecture & Changes

### 2.1 Package Metadata (`Cargo.toml`, `CITATION.cff`)
- Update `Cargo.toml`:
  - `homepage = "https://github.com/UnitedOpen-Source/joulex"`
  - `repository = "https://github.com/UnitedOpen-Source/joulex"`
- Update `CITATION.cff`:
  - `title: joulex`
  - `repository-code: https://github.com/UnitedOpen-Source/joulex`
  - `version: 0.1.0`
  - Authors Matheus Breguêz and David Peter.

### 2.2 CLI & Warning Strings (`src/output/warnings.rs`, `src/cli.rs`)
- `src/output/warnings.rs`: Change "because hyperfine can not calibrate" to "because joulex can not calibrate".
- `src/cli.rs`:
  - Replace user-facing mentions of `hyperfine` with `joulex`.
  - Update example invocation from `hyperfine 'my-command > output-${HYPERFINE_ITERATION}.log'` to `joulex 'my-command > output-${JOULEX_ITERATION}.log'`.

### 2.3 Documentation & Manpages (`README.md`, `doc/joulex.1`)
- Update `README.md`:
  - Update CI badge to `https://github.com/UnitedOpen-Source/joulex/actions/workflows/CICD.yml/badge.svg`.
  - Fix MSRV requirement note to Rust 1.88 or newer.
  - Update cargo install command to `cargo install --locked joulex`.
- Create `doc/joulex.1` (and keep `doc/hyperfine.1` in sync or aliased):
  - Fix `\fi` font escapes to `\fI`.
  - Fix `\-\-time-unit` to `\-\-time\-unit`.
  - Add `-C, \-\-conclude`.
  - Add `--reference` and `--reference-name`.
  - Document `-E, \-\-energy`, `--deep-stats`, `--schedule`, `--import-json`, `-e, \-\-export`, `--filter-failed`, `--omit-failed-runs`, and `--suppress-outlier-warnings`.

---

## 3. Test Plan
- Run `cargo check`, `cargo test`, `cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`.
- Verify `man` formatting with `man ./doc/joulex.1` (or `groff -mandoc -Tutf8 ./doc/joulex.1`).
- Verify CI workflow target `doc/joulex.1` exists.
