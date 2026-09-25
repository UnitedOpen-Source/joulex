//! Per-run tables (`--export-markdown-runs`, `--export-orgmode-runs`,
//! `--export-asciidoc-runs`): one table per benchmark with every timed run.

use super::markup::{determine_unit_from_results, Alignment, MarkupExporter};
use super::Exporter;
use crate::benchmark::benchmark_result::BenchmarkResult;
use crate::options::SortOrder;
use crate::output::format::format_duration_value;
use crate::util::units::{format_bytes, Unit};

use anyhow::Result;

#[derive(Default)]
pub struct RunsExporter<M: MarkupExporter> {
    markup: M,
}

/// The iteration numbers (as in `JOULEX_ITERATION`) of the runs that are
/// still present in `result`, i.e. without runs removed by
/// `--omit-failed-runs` or `--discard-outliers`.
fn iteration_numbers(result: &BenchmarkResult, kept: usize) -> Vec<usize> {
    let removed: Vec<usize> = result
        .omitted_failed_runs
        .iter()
        .map(|run| run.index)
        .chain(result.discarded_outliers.iter().copied())
        .collect();
    (0..kept + removed.len())
        .filter(|index| !removed.contains(index))
        .collect()
}

impl<M: MarkupExporter> RunsExporter<M> {
    fn table(&self, result: &BenchmarkResult, unit: Unit) -> String {
        let times = result.times.as_deref().unwrap_or_default();
        let n = times.len();
        fn per_run(values: Option<&Vec<f64>>, n: usize) -> Option<&Vec<f64>> {
            values.filter(|v| v.len() == n)
        }
        let user = per_run(result.user_times.as_ref(), n);
        let system = per_run(result.system_times.as_ref(), n);
        let energy = per_run(result.energy_joules.as_ref(), n);
        let memory = result.memory_usage_byte.as_ref().filter(|m| m.len() == n);
        let exit_codes = Some(&result.exit_codes).filter(|e| e.len() == n);
        let unit_name = unit.short_name();

        let mut header = vec!["Iteration".to_string(), format!("Wall [{unit_name}]")];
        if user.is_some() && system.is_some() {
            header.push(format!("User [{unit_name}]"));
            header.push(format!("System [{unit_name}]"));
        }
        if memory.is_some() {
            header.push("Peak memory".into());
        }
        if energy.is_some() {
            header.push("Energy [J]".into());
        }
        if exit_codes.is_some() {
            header.push("Exit code".into());
        }
        let alignment: Vec<Alignment> = header.iter().map(|_| Alignment::Right).collect();

        let mut table = self.markup.heading(&result.command_with_unused_parameters);
        table.push_str(&self.markup.table_header(&alignment));
        table.push_str(&self.markup.table_row(&cells(&header)));
        table.push_str(&self.markup.table_divider(&alignment));

        let duration = |v: f64| format_duration_value(v, Some(unit)).0;
        for (i, iteration) in iteration_numbers(result, n).into_iter().enumerate() {
            let mut row = vec![iteration.to_string(), duration(times[i])];
            if let (Some(user), Some(system)) = (user, system) {
                row.push(duration(user[i]));
                row.push(duration(system[i]));
            }
            if let Some(memory) = memory {
                row.push(format_bytes(memory[i]));
            }
            if let Some(energy) = energy {
                row.push(format!("{:.4}", energy[i]));
            }
            if let Some(exit_codes) = exit_codes {
                row.push(exit_codes[i].map_or("signal".into(), |c| c.to_string()));
            }
            table.push_str(&self.markup.table_row(&cells(&row)));
        }
        table.push_str(&self.markup.table_footer(&alignment));
        table
    }
}

fn cells(values: &[String]) -> Vec<&str> {
    values.iter().map(String::as_str).collect()
}

impl<M: MarkupExporter> Exporter for RunsExporter<M> {
    fn serialize(
        &self,
        results: &[BenchmarkResult],
        _reference: Option<&BenchmarkResult>,
        unit: Option<Unit>,
        _sort_order: SortOrder,
    ) -> Result<Vec<u8>> {
        let unit = unit.unwrap_or_else(|| determine_unit_from_results(results));
        let tables: Vec<String> = results
            .iter()
            .filter(|result| result.times.as_ref().is_some_and(|t| !t.is_empty()))
            .map(|result| self.table(result, unit))
            .collect();
        Ok(tables.join("\n").into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::benchmark_result::OmittedRun;
    use crate::export::markdown::MarkdownExporter;
    use crate::export::orgmode::OrgmodeExporter;

    fn result() -> BenchmarkResult {
        BenchmarkResult {
            command: "sleep 0.1".into(),
            command_with_unused_parameters: "sleep 0.1".into(),
            times: Some(vec![0.1, 0.102, 0.098]),
            user_times: Some(vec![0.001, 0.002, 0.001]),
            system_times: Some(vec![0.002, 0.001, 0.002]),
            memory_usage_byte: Some(vec![1_048_576, 1_048_576, 2_097_152]),
            exit_codes: vec![Some(0), Some(0), Some(0)],
            ..Default::default()
        }
    }

    #[test]
    fn markdown_table_with_all_runs() {
        let out = RunsExporter::<MarkdownExporter>::default()
            .serialize(
                &[result()],
                None,
                Some(Unit::MilliSecond),
                SortOrder::Command,
            )
            .unwrap();
        insta::assert_snapshot!(String::from_utf8(out).unwrap(), @r"
        ### `sleep 0.1`

        | Iteration | Wall [ms] | User [ms] | System [ms] | Peak memory | Exit code |
        |---:|---:|---:|---:|---:|---:|
        | 0 | 100.0 | 1.0 | 2.0 | 1.0 MB | 0 |
        | 1 | 102.0 | 2.0 | 1.0 | 1.0 MB | 0 |
        | 2 | 98.0 | 1.0 | 2.0 | 2.0 MB | 0 |
        ");
    }

    #[test]
    fn iteration_numbers_skip_removed_runs() {
        let mut r = result();
        r.omitted_failed_runs = vec![OmittedRun {
            index: 1,
            exit_code: Some(1),
        }];
        r.discarded_outliers = vec![3];
        // 3 kept runs + 2 removed = 5 iterations: 0, 2, 4 were kept
        assert_eq!(iteration_numbers(&r, 3), vec![0, 2, 4]);
    }

    #[test]
    fn orgmode_escapes_the_command_heading() {
        let mut r = result();
        r.command_with_unused_parameters = "a | b".into();
        let out = RunsExporter::<OrgmodeExporter>::default()
            .serialize(&[r], None, Some(Unit::MilliSecond), SortOrder::Command)
            .unwrap();
        assert!(String::from_utf8(out)
            .unwrap()
            .starts_with("* a \\vert{} b\n"));
    }

    #[test]
    fn energy_column_only_with_complete_energy_data() {
        let mut r = result();
        r.energy_joules = Some(vec![0.5, 0.6, 0.7]);
        let out = String::from_utf8(
            RunsExporter::<MarkdownExporter>::default()
                .serialize(
                    &[r.clone()],
                    None,
                    Some(Unit::MilliSecond),
                    SortOrder::Command,
                )
                .unwrap(),
        )
        .unwrap();
        assert!(out.contains("| Energy [J] |") && out.contains("| 0.5000 |"));

        r.energy_joules = Some(vec![0.5]); // incomplete: some samples missing
        let out = String::from_utf8(
            RunsExporter::<MarkdownExporter>::default()
                .serialize(&[r], None, Some(Unit::MilliSecond), SortOrder::Command)
                .unwrap(),
        )
        .unwrap();
        assert!(!out.contains("Energy"));
    }
}
