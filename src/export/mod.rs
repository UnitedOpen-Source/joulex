use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

mod asciidoc;
mod csv;
mod html;
mod json;
mod markdown;
mod markup;
pub mod metadata;
mod orgmode;
mod runs;
#[cfg(test)]
mod tests;

use self::asciidoc::AsciidocExporter;
use self::csv::CsvExporter;
use self::html::HtmlExporter;
use self::json::JsonExporter;
use self::markdown::MarkdownExporter;
use self::metadata::{parse_labels, ExportMetadata};
use self::orgmode::OrgmodeExporter;
use self::runs::RunsExporter;

use crate::benchmark::benchmark_result::BenchmarkResult;
use crate::options::SortOrder;
use crate::util::units::Unit;

use anyhow::{Context, Result};
use clap::ArgMatches;

/// The desired form of exporter to use for a given file.
#[derive(Clone)]
pub enum ExportType {
    /// Asciidoc Table
    Asciidoc,

    /// CSV (comma separated values) format
    Csv,

    /// JSON format
    Json,

    /// Markdown table
    Markdown,

    /// Emacs org-mode tables
    Orgmode,

    /// One Markdown table per benchmark with every run
    MarkdownRuns,

    /// One org-mode table per benchmark with every run
    OrgmodeRuns,

    /// One AsciiDoc table per benchmark with every run
    AsciidocRuns,

    /// Self-contained HTML report with plots
    Html,
}

/// Interface for different exporters.
trait Exporter {
    /// Export the given entries in the serialized form.
    fn serialize(
        &self,
        results: &[BenchmarkResult],
        unit: Option<Unit>,
        sort_order: SortOrder,
    ) -> Result<Vec<u8>>;
}

pub enum ExportTarget {
    File(String),
    Stdout,
}

struct ExporterWithTarget {
    exporter: Box<dyn Exporter>,
    target: ExportTarget,
}

/// Handles the management of multiple file exporters.
pub struct ExportManager {
    exporters: Vec<ExporterWithTarget>,
    time_unit: Option<Unit>,
    sort_order: SortOrder,
    /// Run metadata for the JSON and CSV exports
    metadata: ExportMetadata,
}

impl ExportManager {
    /// Build the ExportManager that will export the results specified
    /// in the given ArgMatches
    pub fn from_cli_arguments(
        matches: &ArgMatches,
        time_unit: Option<Unit>,
        sort_order: SortOrder,
    ) -> Result<Self> {
        let labels = parse_labels(
            matches
                .get_many::<String>("label")
                .into_iter()
                .flatten()
                .map(String::as_str),
        )?;
        let mut export_manager = Self {
            exporters: vec![],
            time_unit,
            sort_order,
            metadata: ExportMetadata::new(labels),
        };

        if let Some(args) = matches.get_many::<String>("export") {
            for filename in args {
                let export_type = get_export_type_from_filename(filename);
                export_manager.add_exporter(export_type, filename)?;
            }
        }

        {
            let mut add_exporter = |flag, exporttype| -> Result<()> {
                if let Some(filename) = matches.get_one::<String>(flag) {
                    export_manager.add_exporter(exporttype, filename)?;
                }
                Ok(())
            };
            add_exporter("export-asciidoc", ExportType::Asciidoc)?;
            add_exporter("export-json", ExportType::Json)?;
            add_exporter("export-csv", ExportType::Csv)?;
            add_exporter("export-markdown", ExportType::Markdown)?;
            add_exporter("export-orgmode", ExportType::Orgmode)?;
            add_exporter("export-markdown-runs", ExportType::MarkdownRuns)?;
            add_exporter("export-orgmode-runs", ExportType::OrgmodeRuns)?;
            add_exporter("export-asciidoc-runs", ExportType::AsciidocRuns)?;
            add_exporter("export-html", ExportType::Html)?;
        }
        Ok(export_manager)
    }

    /// Add an additional exporter to the ExportManager
    pub fn add_exporter(&mut self, export_type: ExportType, filename: &str) -> Result<()> {
        let exporter: Box<dyn Exporter> = match export_type {
            ExportType::Asciidoc => Box::<AsciidocExporter>::default(),
            ExportType::Csv => Box::new(CsvExporter {
                labels: self.metadata.labels.clone(),
            }),
            ExportType::Json => Box::new(JsonExporter {
                metadata: Some(self.metadata.clone()),
            }),
            ExportType::Markdown => Box::<MarkdownExporter>::default(),
            ExportType::Orgmode => Box::<OrgmodeExporter>::default(),
            ExportType::MarkdownRuns => Box::<RunsExporter<MarkdownExporter>>::default(),
            ExportType::OrgmodeRuns => Box::<RunsExporter<OrgmodeExporter>>::default(),
            ExportType::AsciidocRuns => Box::<RunsExporter<AsciidocExporter>>::default(),
            ExportType::Html => Box::<HtmlExporter>::default(),
        };

        self.exporters.push(ExporterWithTarget {
            exporter,
            target: if filename == "-" {
                ExportTarget::Stdout
            } else {
                // Fail early (before any benchmark runs) if the file can never be
                // written, but do not create or truncate it yet: an existing
                // export is only replaced once new results are available.
                check_export_target(filename)
                    .with_context(|| format!("Could not create export file '{filename}'"))?;
                ExportTarget::File(filename.to_string())
            },
        });

        Ok(())
    }

    /// Write the given results to all Exporters. The 'intermediate' flag specifies
    /// whether this is being called while still performing benchmarks, or if this
    /// is the final call after all benchmarks have been finished.
    ///
    /// Regular files are (re)written on every call, so that they are always up to
    /// date, even if a later benchmark fails. Stdout targets (`-`) and special
    /// files such as /dev/stdout or FIFOs are only written by the final call:
    /// writes to them accumulate instead of replacing each other.
    pub fn write_results(&self, results: &[BenchmarkResult], intermediate: bool) -> Result<()> {
        for e in &self.exporters {
            let content = || {
                e.exporter
                    .serialize(results, self.time_unit, self.sort_order)
            };

            match e.target {
                ExportTarget::File(ref filename) => {
                    if !(intermediate && is_special_file(Path::new(filename))) {
                        write_to_file(filename, &content()?)?;
                    }
                }
                ExportTarget::Stdout => {
                    if !intermediate {
                        println!();
                        let content = String::from_utf8(content()?)
                            .context("Export produced invalid UTF-8")?;
                        println!("{content}");
                    }
                }
            }
        }
        Ok(())
    }
}

/// Check that `filename` can later be written by `write_to_file`: its parent
/// directory must exist and it must not be a directory itself.
fn check_export_target(filename: &str) -> std::io::Result<()> {
    let path = Path::new(filename);
    if path.is_dir() {
        return Err(std::io::Error::other("is a directory"));
    }
    let dir = parent_dir(path);
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No such file or directory",
        ));
    }
    Ok(())
}

/// True if `path` exists and (after following symlinks) is neither a regular
/// file nor a directory, e.g. a character device, FIFO or socket.
fn is_special_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| !m.is_file() && !m.is_dir())
}

fn parent_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Atomically replace `filename` with `content`: write to a temporary file in
/// the same directory, then rename it over the target. Readers never observe a
/// truncated or partially written export, an existing file is left untouched
/// if writing fails, and a symlink at `filename` is replaced instead of its
/// target being overwritten.
fn write_to_file(filename: &str, content: &[u8]) -> Result<()> {
    let path = Path::new(filename);

    // Devices, FIFOs and other special files (e.g. /dev/stdout, /dev/null or
    // a `>(…)` process substitution) can't be replaced by a rename: write to
    // them directly. Only regular files (or new paths) are replaced atomically.
    if is_special_file(path) {
        return OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|mut file| file.write_all(content))
            .with_context(|| format!("Failed to export results to '{filename}'"));
    }

    let file_name = path
        .file_name()
        .with_context(|| format!("Invalid export file name '{filename}'"))?;
    let mut tmp_name = OsStr::new(".").to_os_string();
    tmp_name.push(file_name);
    tmp_name.push(format!(".joulex-tmp-{}", std::process::id()));
    let tmp_path = parent_dir(path).join(tmp_name);

    let write_tmp = || -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        file.write_all(content)?;
        file.sync_all()?;
        fs::rename(&tmp_path, path)
    };

    write_tmp()
        .inspect_err(|_| {
            let _ = fs::remove_file(&tmp_path);
        })
        .with_context(|| format!("Failed to export results to '{filename}'"))
}

/// Determine the export-type from the file extension. Defaults to JSON.
fn get_export_type_from_filename(filename: &str) -> ExportType {
    match Path::new(filename)
        .extension()
        .and_then(OsStr::to_str)
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("adoc" | "asciidoc") => ExportType::Asciidoc,
        Some("csv") => ExportType::Csv,
        Some("md" | "markdown") => ExportType::Markdown,
        Some("org") => ExportType::Orgmode,
        Some("html" | "htm") => ExportType::Html,
        _ => ExportType::Json,
    }
}
