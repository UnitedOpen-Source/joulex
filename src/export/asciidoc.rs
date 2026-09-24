use super::markup::Alignment;
use crate::export::markup::MarkupExporter;

#[derive(Default)]
pub struct AsciidocExporter {}

impl MarkupExporter for AsciidocExporter {
    fn heading(&self, cmd: &str) -> String {
        format!("=== {}\n", self.command(cmd))
    }

    fn table_header(&self, cell_aligmnents: &[Alignment]) -> String {
        format!(
            "[cols=\"{}\"]\n|===",
            cell_aligmnents
                .iter()
                .map(|a| match a {
                    Alignment::Left => "<",
                    Alignment::Right => ">",
                })
                .collect::<Vec<&str>>()
                .join(",")
        )
    }

    fn table_footer(&self, _cell_aligmnents: &[Alignment]) -> String {
        "|===\n".to_string()
    }

    fn table_row(&self, cells: &[&str]) -> String {
        format!("\n| {} \n", cells.join(" \n| "))
    }

    fn table_divider(&self, _cell_aligmnents: &[Alignment]) -> String {
        "".to_string()
    }

    fn command(&self, cmd: &str) -> String {
        // Use AsciiDoc's built-in character attributes so that the content can
        // neither close the monospace span (`) nor start a new cell (|).
        // `<`, `>` and `&` are already escaped by AsciiDoc's special-character
        // substitution.
        let cmd = cmd.replace('`', "{backtick}").replace('|', "{vbar}");
        format!("`{cmd}`")
    }
}

/// Check Asciidoc-based data row formatting
#[test]
fn test_asciidoc_exporter_table_data() {
    let exporter = AsciidocExporter::default();
    let data = vec!["a", "b", "c"];

    let actual = exporter.table_row(&data);
    let expect = "\n| a \n| b \n| c \n";

    assert_eq!(expect, actual);
}

/// Check Asciidoc-based table header formatting
#[test]
fn test_asciidoc_exporter_table_header() {
    let exporter = AsciidocExporter::default();
    let cells_alignment = [
        Alignment::Left,
        Alignment::Right,
        Alignment::Right,
        Alignment::Right,
        Alignment::Right,
    ];

    let actual = exporter.table_header(&cells_alignment);
    let expect = "[cols=\"<,>,>,>,>\"]\n|===";

    assert_eq!(expect, actual);
}

/// Commands must not be able to close the monospace span or the table cell (#25)
#[test]
fn test_asciidoc_command_escaping() {
    let exporter = AsciidocExporter::default();

    assert_eq!(exporter.command("sleep 1"), "`sleep 1`");
    assert_eq!(exporter.command("x`y|z"), "`x{backtick}y{vbar}z`");
}
