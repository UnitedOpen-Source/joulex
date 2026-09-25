# Spec: Review Follow-up Refinements (Windows Palette, Energy Alignment, UX Nits)

## 1. Context & Motivation
Following Code Review #113 (Follow-ups 1, 3, and 4), several technical and UX improvements were identified:
1. **Windows Console Theme Detection & Palette (Follow-up 4)**:
   - In Windows PowerShell 5.1, the default background color uses palette slot 5 (`DarkMagenta`, `0x50` in `wAttributes`), remapped to dark blue RGB(1, 36, 86). `from_console_attributes` previously only matched `0x10`.
   - The Windows build has `#![warn(clippy::undocumented_unsafe_blocks)]`, requiring explicit `// SAFETY:` comments on Win32 API calls in `src/output/colors.rs`.
   - Remapping `Green -> Yellow` and `Yellow -> BrightYellow` caused *mean* and *median* colors to collapse. Only `Blue` and `Magenta`/`Purple` are unreadable on a dark blue background.
   - `GetStdHandle` returns `INVALID_HANDLE_VALUE` on failure, which must be checked alongside `is_null()`.
2. **Energy Vector 1:1 Run Alignment (#125)**:
   - When RAPL measurements fail for a subset of runs, building `energy_joules` with `.flatten()` produces a vector shorter than `times`. While `--export-runs` checks length, JSON consumers experience index misalignment. The incomplete per-run vector should be omitted (`None`) while preserving aggregate metrics (`mean_energy_joules` and `mean_watts`).
3. **UX Refinements (#108, #142, #167)**:
   - In `--sort=command` speed comparisons with `--reference`, the numeric ratio was printed twice (in the first column and inside the trailing annotation).
   - CLI help for `--allow-setup-with-round-robin` should document that setup runs once per batch of runs, not before each interleaved iteration.
   - Flag sprawl for off-CPU warnings: hide `--no-off-cpu-warning` from `--help` in favor of `--suppress-warnings off-cpu`.

## 2. Detailed Design

### 2.1 Windows Console Palette (`src/output/colors.rs`)
- Add `const POWERSHELL_SLOT5_BACKGROUND: u16 = 0x0050;`.
- In `from_console_attributes(attributes: u16)`:
  ```rust
  let bg = attributes & BACKGROUND_COLOR_MASK;
  if bg == BLUE_BACKGROUND || bg == POWERSHELL_SLOT5_BACKGROUND {
      Self::LegacyWindowsConsole
  } else {
      Self::Default
  }
  ```
- In `Theme::LegacyWindowsConsole::color()`:
  - `SemanticColor::Green => Color::Green`
  - `SemanticColor::Blue => Color::Cyan`
  - `SemanticColor::Cyan => Color::Cyan`
  - `SemanticColor::Purple => Color::BrightRed`
  - `SemanticColor::Magenta => Color::BrightRed`
  - `SemanticColor::Yellow => Color::Yellow`
  - `SemanticColor::Red => Color::Red`
- In `detect()`:
  - Add `// SAFETY:` comments.
  - Check `handle.is_null() || handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE`.

### 2.2 Energy Measurement Alignment (`src/benchmark/mod.rs`)
- When computing `energy_all`:
  ```rust
  let valid_energy: Vec<f64> = self.energy_measurements.into_iter().flatten().collect();
  let energy_all = if valid_energy.len() == self.times_real.len() {
      Some(valid_energy.clone())
  } else {
      None
  };
  let (mean_energy, mean_watts) = if !valid_energy.is_empty() {
      let m_j = mean(&valid_energy);
      let m_w = if t_mean > 0.0 { m_j / t_mean } else { 0.0 };
      (Some(m_j), Some(m_w))
  } else {
      (None, None)
  };
  ```

### 2.3 UX Improvements
- `src/benchmark/scheduler.rs`:
  - Change `"  {:.2} times faster than {ref_cmd}"` -> `"  faster than {ref_cmd}"`.
  - Change `"  {:.2} times slower than {ref_cmd}"` -> `"  slower than {ref_cmd}"`.
  - Retain `"  as fast as {ref_cmd}"`.
- `src/cli.rs`:
  - Clarify `--allow-setup-with-round-robin` help text.
  - Add `.hide(true)` to `--no-off-cpu-warning`.

## 3. Testing & Verification
- Unit tests in `src/output/colors.rs` covering:
  - `0x0010` and `0x0050` selecting `Theme::LegacyWindowsConsole`.
  - Default selection for non-blue backgrounds.
  - Preservation of green/yellow/cyan and mapping of blue/magenta/purple.
- Unit tests for energy alignment verifying that incomplete energy measurements yield `energy_joules: None` with valid `mean_energy_joules`.
- Integration tests for sort command speed comparison without redundant ratio.
