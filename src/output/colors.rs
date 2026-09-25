use std::fmt::Display;
use std::sync::OnceLock;

use colored::{Color, ColoredString, Colorize};

#[cfg(any(windows, test))]
const BACKGROUND_COLOR_MASK: u16 = 0x00f0;
#[cfg(any(windows, test))]
const BLUE_BACKGROUND: u16 = 0x0010;
#[cfg(any(windows, test))]
const POWERSHELL_SLOT5_BACKGROUND: u16 = 0x0050;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Theme {
    Default,
    #[cfg_attr(not(any(windows, test)), allow(dead_code))]
    LegacyWindowsConsole,
}

#[derive(Clone, Copy)]
enum SemanticColor {
    Green,
    Blue,
    Cyan,
    Purple,
    Magenta,
    Yellow,
    Red,
}

impl Theme {
    #[cfg(any(windows, test))]
    fn from_console_attributes(attributes: u16) -> Self {
        let bg = attributes & BACKGROUND_COLOR_MASK;
        if bg == BLUE_BACKGROUND || bg == POWERSHELL_SLOT5_BACKGROUND {
            Self::LegacyWindowsConsole
        } else {
            Self::Default
        }
    }

    fn color(self, semantic_color: SemanticColor) -> Color {
        match self {
            Self::Default => match semantic_color {
                SemanticColor::Green => Color::Green,
                SemanticColor::Blue => Color::Blue,
                SemanticColor::Cyan => Color::Cyan,
                SemanticColor::Purple => Color::Magenta,
                SemanticColor::Magenta => Color::Magenta,
                SemanticColor::Yellow => Color::Yellow,
                SemanticColor::Red => Color::Red,
            },
            Self::LegacyWindowsConsole => match semantic_color {
                SemanticColor::Green => Color::Green,
                SemanticColor::Blue => Color::Cyan,
                SemanticColor::Cyan => Color::Cyan,
                SemanticColor::Purple => Color::BrightRed,
                SemanticColor::Magenta => Color::BrightRed,
                SemanticColor::Yellow => Color::Yellow,
                SemanticColor::Red => Color::Red,
            },
        }
    }

    fn paint(self, text: impl Display, semantic_color: SemanticColor) -> ColoredString {
        text.to_string().color(self.color(semantic_color))
    }
}

fn detect() -> Theme {
    #[cfg(windows)]
    {
        use std::mem::MaybeUninit;

        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::Console::{
            GetConsoleScreenBufferInfo, GetStdHandle, CONSOLE_SCREEN_BUFFER_INFO, STD_OUTPUT_HANDLE,
        };

        // SAFETY: GetStdHandle has no preconditions and is safe to call with standard handle constants.
        let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Theme::Default;
        }

        let mut info = MaybeUninit::<CONSOLE_SCREEN_BUFFER_INFO>::uninit();
        // SAFETY: `info.as_mut_ptr()` points to valid writable uninitialized memory for `CONSOLE_SCREEN_BUFFER_INFO`.
        if unsafe { GetConsoleScreenBufferInfo(handle, info.as_mut_ptr()) } != 0 {
            // SAFETY: `info` is guaranteed to be fully initialized when `GetConsoleScreenBufferInfo` succeeds (returns non-zero).
            return Theme::from_console_attributes(unsafe { info.assume_init() }.wAttributes);
        }
    }

    Theme::Default
}

fn current_theme() -> Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    *THEME.get_or_init(detect)
}

pub fn green(text: impl Display) -> ColoredString {
    current_theme().paint(text, SemanticColor::Green)
}

pub fn blue(text: impl Display) -> ColoredString {
    current_theme().paint(text, SemanticColor::Blue)
}

pub fn cyan(text: impl Display) -> ColoredString {
    current_theme().paint(text, SemanticColor::Cyan)
}

pub fn purple(text: impl Display) -> ColoredString {
    current_theme().paint(text, SemanticColor::Purple)
}

pub fn magenta(text: impl Display) -> ColoredString {
    current_theme().paint(text, SemanticColor::Magenta)
}

pub fn yellow(text: impl Display) -> ColoredString {
    current_theme().paint(text, SemanticColor::Yellow)
}

pub fn red(text: impl Display) -> ColoredString {
    current_theme().paint(text, SemanticColor::Red)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_windows_console_uses_readable_colors() {
        for bg in [BLUE_BACKGROUND, POWERSHELL_SLOT5_BACKGROUND] {
            let theme = Theme::from_console_attributes(bg);

            assert_eq!(theme, Theme::LegacyWindowsConsole);
            assert_eq!(theme.color(SemanticColor::Green), Color::Green);
            assert_eq!(theme.color(SemanticColor::Blue), Color::Cyan);
            assert_eq!(theme.color(SemanticColor::Cyan), Color::Cyan);
            assert_eq!(theme.color(SemanticColor::Purple), Color::BrightRed);
            assert_eq!(theme.color(SemanticColor::Magenta), Color::BrightRed);
            assert_eq!(theme.color(SemanticColor::Yellow), Color::Yellow);
            assert_eq!(theme.color(SemanticColor::Red), Color::Red);
        }
    }

    #[test]
    fn non_blue_background_selects_default_theme() {
        assert_eq!(Theme::from_console_attributes(0), Theme::Default);
        assert_eq!(
            Theme::from_console_attributes(BLUE_BACKGROUND | 0x0080),
            Theme::Default
        );
        assert_eq!(
            Theme::from_console_attributes(POWERSHELL_SLOT5_BACKGROUND | 0x0080),
            Theme::Default
        );
        assert_eq!(Theme::from_console_attributes(0x0020), Theme::Default);
    }
}
