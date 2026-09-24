//! Helpers for displaying untrusted text (e.g. command names read from an
//! imported JSON file) without letting it control the terminal.

use std::borrow::Cow;

/// Replace control characters (C0, DEL and C1, which includes ESC) by a visible
/// `\u{..}` escape, so that the text cannot inject ANSI/OSC sequences into the
/// terminal or into exported reports (CWE-150).
///
/// Strings without control characters are returned unchanged (borrowed).
pub fn escape_control_chars(s: &str) -> Cow<'_, str> {
    if !s.chars().any(char::is_control) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if c.is_control() {
            out.push_str(&format!("\\u{{{:x}}}", c as u32));
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_plain_text_untouched() {
        assert!(matches!(
            escape_control_chars("sleep 0.1 | wc -l"),
            Cow::Borrowed("sleep 0.1 | wc -l")
        ));
        assert_eq!(escape_control_chars("naïve ✓ 日本"), "naïve ✓ 日本");
    }

    #[test]
    fn escapes_ansi_and_osc_sequences() {
        assert_eq!(
            escape_control_chars("\u{1b}]0;PWNED\u{7}\u{1b}[2Jx"),
            "\\u{1b}]0;PWNED\\u{7}\\u{1b}[2Jx"
        );
    }

    #[test]
    fn escapes_c1_and_del() {
        assert_eq!(escape_control_chars("a\u{9b}b\u{7f}"), "a\\u{9b}b\\u{7f}");
    }

    #[test]
    fn escapes_newlines_and_tabs() {
        assert_eq!(escape_control_chars("a\nb\tc"), "a\\u{a}b\\u{9}c");
    }
}
