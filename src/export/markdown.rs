use crate::export::markup::MarkupExporter;

use super::markup::Alignment;

#[derive(Default)]
pub struct MarkdownExporter {}

impl MarkupExporter for MarkdownExporter {
    fn heading(&self, cmd: &str) -> String {
        format!("### {}\n\n", self.command(cmd))
    }

    fn table_row(&self, cells: &[&str]) -> String {
        format!("| {} |\n", cells.join(" | "))
    }

    fn table_divider(&self, cell_aligmnents: &[Alignment]) -> String {
        format!(
            "|{}\n",
            cell_aligmnents
                .iter()
                .map(|a| match a {
                    Alignment::Left => ":---|",
                    Alignment::Right => "---:|",
                })
                .collect::<String>()
        )
    }

    fn command(&self, cmd: &str) -> String {
        // GitHub-flavored Markdown needs `|` escaped even inside code spans
        // within a table.
        let cmd = cmd.replace('|', "\\|");

        // CommonMark: a code span is delimited by a backtick string that is
        // longer than any run of backticks in its content, so the content
        // cannot close the span and inject Markdown/HTML.
        let longest_run = cmd.split(|c| c != '`').map(str::len).max().unwrap_or(0);
        let fence = "`".repeat(longest_run + 1);

        // Content starting or ending with a backtick needs a padding space,
        // which CommonMark strips again.
        if cmd.starts_with('`') || cmd.ends_with('`') {
            format!("{fence} {cmd} {fence}")
        } else {
            format!("{fence}{cmd}{fence}")
        }
    }
}

/// Check Markdown-based data row formatting
#[test]
fn test_markdown_formatter_table_data() {
    let formatter = MarkdownExporter::default();

    assert_eq!(formatter.table_row(&["a", "b", "c"]), "| a | b | c |\n");
}

/// Check Markdown-based horizontal line formatting
#[test]
fn test_markdown_formatter_table_divider() {
    let formatter = MarkdownExporter::default();

    let divider = formatter.table_divider(&[Alignment::Left, Alignment::Right, Alignment::Left]);
    assert_eq!(divider, "|:---|---:|:---|\n");
}

/// Commands must not be able to close the code span or the table cell (#25)
#[test]
fn test_markdown_command_escaping() {
    let formatter = MarkdownExporter::default();

    assert_eq!(formatter.command("sleep 1"), "`sleep 1`");
    assert_eq!(formatter.command("a | b"), "`a \\| b`");
    assert_eq!(
        formatter.command("x`<img src=x onerror=alert(1)>`y"),
        "``x`<img src=x onerror=alert(1)>`y``"
    );
    assert_eq!(formatter.command("a ``` b"), "````a ``` b````");
    assert_eq!(formatter.command("`start"), "`` `start ``");
}
