# Spec: Readable Colors on Legacy Windows Consoles

## 1. Problem Statement
In legacy Windows consoles (such as standard PowerShell and legacy `cmd.exe`/conhost) with the default dark-blue background (`0x0010`), blue and magenta output rendered by terminal applications is difficult or impossible to read.

This addresses the final part (Item 3) of Issue #68. Items 1 and 2 (relative forward-slash paths under `cmd.exe` and backslash paths with `-N`) were completed in PR #177.

## 2. Design and Architecture

### 2.1 Color Abstraction (`src/output/colors.rs`)
Introduce semantic color functions wrapping `colored::ColoredString`:
- `colors::green(text)`
- `colors::blue(text)`
- `colors::cyan(text)`
- `colors::purple(text)`
- `colors::magenta(text)`
- `colors::yellow(text)`
- `colors::red(text)`

### 2.2 Console Background Detection
On Windows targets:
- Inspect `STD_OUTPUT_HANDLE` via `GetStdHandle`.
- Query `CONSOLE_SCREEN_BUFFER_INFO` using `GetConsoleScreenBufferInfo`.
- Extract `wAttributes & 0x00f0` (background color mask).
- If background color equals `0x0010` (`BACKGROUND_BLUE` without `BACKGROUND_INTENSITY`), activate `Theme::LegacyWindowsConsole`.
- Cache detection result using `std::sync::OnceLock<Theme>`.

### 2.3 Semantic Color Mapping
In `Theme::LegacyWindowsConsole`:
- `Green` -> `Color::Yellow`
- `Blue` -> `Color::Cyan`
- `Cyan` -> `Color::BrightGreen`
- `Purple` -> `Color::BrightRed`
- `Magenta` -> `Color::Cyan`
- `Yellow` -> `Color::BrightYellow`
- `Red` -> `Color::BrightRed`

In `Theme::Default` (Unix and modern Windows terminals):
- Standard colors corresponding to their semantic names (`Green`, `Blue`, `Cyan`, `Magenta`, `Yellow`, `Red`).

### 2.4 Dependencies
Add `"Win32_System_Console"` to `target.'cfg(windows)'.dependencies.windows-sys.features` in `Cargo.toml`.

## 3. Verification & Testing
- Unit tests for theme determination from console attribute bitmask.
- Unit tests for color resolution in both default and legacy console themes.
- Regression testing on Unix/macOS to ensure standard terminal output remains identical.
- Cross-target compilation checks (`x86_64-pc-windows-gnu` / Windows targets).
