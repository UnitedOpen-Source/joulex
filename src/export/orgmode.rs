use super::markup::Alignment;
use crate::export::markup::MarkupExporter;

#[derive(Default)]
pub struct OrgmodeExporter {}

impl MarkupExporter for OrgmodeExporter {
    fn table_row(&self, cells: &[&str]) -> String {
        format!(
            "| {}  |  {} |\n",
            cells.first().unwrap(),
            cells[1..].join(" |  ")
        )
    }

    fn table_divider(&self, cell_aligmnents: &[Alignment]) -> String {
        format!("|{}--|\n", "--+".repeat(cell_aligmnents.len() - 1))
    }

    fn command(&self, cmd: &str) -> String {
        // In Org tables a `|` always starts a new cell, even inside markup, so
        // it has to be written as \vert{}.
        let escaped = cmd.replace('|', "\\vert{}");

        // `=verbatim=` can be closed early by an `=` inside the text, and it
        // doesn't render with leading/trailing whitespace. In those cases,
        // fall back to plain text instead of producing broken markup.
        let verbatim_safe = !cmd.is_empty()
            && !cmd.contains(['=', '|'])
            && !cmd.starts_with(char::is_whitespace)
            && !cmd.ends_with(char::is_whitespace);
        if verbatim_safe {
            format!("={escaped}=")
        } else {
            escaped
        }
    }
}

/// Check Emacs org-mode data row formatting
#[test]
fn test_orgmode_formatter_table_data() {
    let exporter = OrgmodeExporter::default();

    let actual = exporter.table_row(&["a", "b", "c"]);
    let expect = "| a  |  b |  c |\n";

    assert_eq!(expect, actual);
}

/// Check Emacs org-mode horizontal line formatting
#[test]
fn test_orgmode_formatter_table_line() {
    let exporter = OrgmodeExporter::default();

    let actual = exporter.table_divider(&[
        Alignment::Left,
        Alignment::Left,
        Alignment::Left,
        Alignment::Left,
        Alignment::Left,
    ]);
    let expect = "|--+--+--+--+--|\n";

    assert_eq!(expect, actual);
}

/// Commands must not be able to close the verbatim markup or the table cell (#25)
#[test]
fn test_orgmode_command_escaping() {
    let exporter = OrgmodeExporter::default();

    assert_eq!(exporter.command("sleep 1"), "=sleep 1=");
    assert_eq!(exporter.command("a | b"), "a \\vert{} b");
    assert_eq!(exporter.command("x=1 y"), "x=1 y");
    assert_eq!(exporter.command(" padded"), " padded");
}
