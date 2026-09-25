use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use csv::WriterBuilder;

use super::Exporter;
use crate::benchmark::benchmark_result::BenchmarkResult;
use crate::benchmark::relative_speed;
use crate::options::SortOrder;
use crate::util::units::Unit;

use anyhow::Result;

#[derive(Default)]
pub struct CsvExporter {
    /// `--label KEY=VALUE` pairs, exported as `label_KEY` columns
    pub labels: BTreeMap<String, String>,
}

/// Sanitizes a CSV field value to prevent formula injection (CWE-1236).
///
/// Spreadsheet applications (Excel, LibreOffice Calc, Google Sheets) interpret
/// cells beginning with `=`, `+`, `-`, or `@` as formulas. When the value
/// originates from user-controlled input (e.g. a benchmarked command name or
/// parameter value), this can lead to formula injection. Prefixing such values
/// with a single quote (`'`) instructs spreadsheet applications to treat the
/// content as literal text.
fn sanitize_csv_value(value: &str) -> Cow<'_, [u8]> {
    match value.chars().next() {
        Some('=') | Some('+') | Some('-') | Some('@') | Some('\t') | Some('\r') => {
            let mut sanitized = String::from("'");
            sanitized.push_str(value);
            Cow::Owned(sanitized.into_bytes())
        }
        _ => Cow::Borrowed(value.as_bytes()),
    }
}

impl Exporter for CsvExporter {
    fn serialize(
        &self,
        results: &[BenchmarkResult],
        reference: Option<&BenchmarkResult>,
        _unit: Option<Unit>,
        _sort_order: SortOrder,
    ) -> Result<Vec<u8>> {
        let mut writer = WriterBuilder::new().from_writer(vec![]);

        let mut all_param_names = BTreeSet::new();
        for res in results {
            for param_name in res.parameters.keys() {
                all_param_names.insert(param_name.as_str());
            }
        }

        {
            let mut headers: Vec<Cow<[u8]>> = [
                // The list of times and exit codes cannot be exported to the CSV file - omit them.
                "command", "mean", "stddev", "median", "user", "system", "min", "max",
            ]
            .iter()
            .map(|x| Cow::Borrowed(x.as_bytes()))
            .collect();

            for param_name in &all_param_names {
                headers.push(Cow::Owned(format!("parameter_{param_name}").into_bytes()));
            }

            // New columns are appended at the end, so that the position of the
            // existing columns doesn't change for consumers that read by index.
            headers.push(Cow::Borrowed(b"relative_speed"));
            headers.push(Cow::Borrowed(b"relative_speed_stddev"));
            for key in self.labels.keys() {
                headers.push(Cow::Owned(format!("label_{key}").into_bytes()));
            }

            writer.write_record(headers)?;
        }

        let relative = if results.is_empty() {
            None
        } else {
            let baseline = reference.unwrap_or_else(|| relative_speed::fastest_of(results));
            relative_speed::compute_with_check_from_reference(results, baseline, SortOrder::Command)
        };

        for (i, res) in results.iter().enumerate() {
            let mut fields = vec![sanitize_csv_value(&res.command)];
            for f in &[
                res.mean,
                res.stddev.unwrap_or(0.0),
                res.median,
                res.user,
                res.system,
                res.min,
                res.max,
            ] {
                fields.push(Cow::Owned(f.to_string().into_bytes()))
            }
            for param_name in &all_param_names {
                let val = res
                    .parameters
                    .get(*param_name)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                fields.push(sanitize_csv_value(val));
            }
            let entry = relative.as_ref().map(|r| &r[i]);
            let optional = |value: Option<f64>| {
                Cow::Owned(
                    value
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                        .into_bytes(),
                )
            };
            fields.push(optional(
                entry.map(|e| e.relative_speed).filter(|r| r.is_finite()),
            ));
            fields.push(optional(
                entry
                    .filter(|e| !e.is_reference)
                    .and_then(|e| e.relative_speed_stddev),
            ));
            for value in self.labels.values() {
                fields.push(sanitize_csv_value(value));
            }
            writer.write_record(fields)?;
        }

        Ok(writer.into_inner()?)
    }
}

#[test]
fn test_csv() {
    use std::collections::BTreeMap;
    let exporter = CsvExporter::default();

    let results = vec![
        BenchmarkResult {
            command: String::from("command_a"),
            command_with_unused_parameters: String::from("command_a"),
            mean: 1.0,
            stddev: Some(2.0),
            median: 1.0,
            user: 3.0,
            system: 4.0,
            cpu_percent: None,
            min: 5.0,
            max: 6.0,
            times: Some(vec![7.0, 8.0, 9.0]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: {
                let mut params = BTreeMap::new();
                params.insert("foo".into(), "one".into());
                params.insert("bar".into(), "two".into());
                params
            },
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
            percentiles: None,
            geometric_mean: None,
            warmup_runs: None,
            runs_planned: None,
            first_run: None,
            diagnostics: None,
            per_run_parameters: None,
        },
        BenchmarkResult {
            command: String::from("command_b"),
            command_with_unused_parameters: String::from("command_b"),
            mean: 11.0,
            stddev: Some(12.0),
            median: 11.0,
            user: 13.0,
            system: 14.0,
            cpu_percent: None,
            min: 15.0,
            max: 16.5,
            times: Some(vec![17.0, 18.0, 19.0]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: {
                let mut params = BTreeMap::new();
                params.insert("foo".into(), "one".into());
                params.insert("bar".into(), "seven".into());
                params
            },
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
            percentiles: None,
            geometric_mean: None,
            warmup_runs: None,
            runs_planned: None,
            first_run: None,
            diagnostics: None,
            per_run_parameters: None,
        },
    ];

    let actual = String::from_utf8(
        exporter
            .serialize(&results, None, Some(Unit::Second), SortOrder::Command)
            .unwrap(),
    )
    .unwrap();

    insta::assert_snapshot!(actual, @r#"
    command,mean,stddev,median,user,system,min,max,parameter_bar,parameter_foo,relative_speed,relative_speed_stddev
    command_a,1,2,1,3,4,5,6,two,one,1,
    command_b,11,12,11,13,14,15,16.5,seven,one,11,25.059928172283335
    "#);
}

#[test]
fn test_csv_formula_injection_sanitization() {
    use std::collections::BTreeMap;
    let exporter = CsvExporter::default();

    let results = vec![BenchmarkResult {
        command: String::from("sleep 0.1"),
        command_with_unused_parameters: String::from("sleep 0.1"),
        mean: 0.1,
        stddev: None,
        median: 0.1,
        user: 0.0,
        system: 0.0,
        cpu_percent: None,
        min: 0.1,
        max: 0.1,
        times: None,
        user_times: None,
        system_times: None,
        memory_usage_byte: None,
        mean_energy_joules: None,
        mean_watts: None,
        energy_joules: None,
        exit_codes: vec![Some(0)],
        parameters: {
            let mut params = BTreeMap::new();
            params.insert("payload".into(), "=1+1".into());
            params.insert("safe_param".into(), "value".into());
            params
        },
        omitted_failed_runs: Vec::new(),
        discarded_outliers: Vec::new(),
        resources: None,
        percentiles: None,
        geometric_mean: None,
        warmup_runs: None,
        runs_planned: None,
        first_run: None,
        diagnostics: None,
        per_run_parameters: None,
    }];

    let actual = String::from_utf8(
        exporter
            .serialize(&results, None, Some(Unit::Second), SortOrder::Command)
            .unwrap(),
    )
    .unwrap();

    insta::assert_snapshot!(actual, @r#"
    command,mean,stddev,median,user,system,min,max,parameter_payload,parameter_safe_param,relative_speed,relative_speed_stddev
    sleep 0.1,0.1,0,0.1,0,0,0.1,0.1,'=1+1,value,1,
    "#);
}

#[test]
fn test_sanitize_csv_value() {
    // Values that begin with formula characters should be prefixed with a single quote
    assert_eq!(
        sanitize_csv_value("=cmd|' /C calc'!A0").as_ref(),
        b"'=cmd|' /C calc'!A0".as_slice()
    );
    assert_eq!(sanitize_csv_value("+1+1").as_ref(), b"'+1+1".as_slice());
    assert_eq!(sanitize_csv_value("-1+1").as_ref(), b"'-1+1".as_slice());
    assert_eq!(
        sanitize_csv_value("@SUM(A1:A5)").as_ref(),
        b"'@SUM(A1:A5)".as_slice()
    );
    assert_eq!(
        sanitize_csv_value("\tformula").as_ref(),
        b"'\tformula".as_slice()
    );
    assert_eq!(
        sanitize_csv_value("\rformula").as_ref(),
        b"'\rformula".as_slice()
    );

    // Safe values should pass through unchanged
    assert_eq!(
        sanitize_csv_value("echo hello").as_ref(),
        "echo hello".as_bytes()
    );
    assert_eq!(sanitize_csv_value("value").as_ref(), b"value".as_slice());
    assert_eq!(sanitize_csv_value("").as_ref(), b"".as_slice());
    assert_eq!(
        sanitize_csv_value("normal-param").as_ref(),
        b"normal-param".as_slice()
    );
}

#[test]
fn test_csv_with_reference_command() {
    use std::collections::BTreeMap;
    let exporter = CsvExporter::default();

    let results = vec![
        BenchmarkResult {
            command: String::from("ref_cmd"),
            command_with_unused_parameters: String::from("ref_cmd"),
            mean: 1.0,
            stddev: None,
            median: 1.0,
            user: 0.5,
            system: 0.5,
            cpu_percent: None,
            min: 1.0,
            max: 1.0,
            times: None,
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0)],
            parameters: BTreeMap::new(),
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
            percentiles: None,
            geometric_mean: None,
            warmup_runs: None,
            runs_planned: None,
            first_run: None,
            diagnostics: None,
            per_run_parameters: None,
        },
        BenchmarkResult {
            command: String::from("param_cmd"),
            command_with_unused_parameters: String::from("param_cmd"),
            mean: 2.0,
            stddev: None,
            median: 2.0,
            user: 1.0,
            system: 1.0,
            cpu_percent: None,
            min: 2.0,
            max: 2.0,
            times: None,
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0)],
            parameters: {
                let mut params = BTreeMap::new();
                params.insert("secs".into(), "2".into());
                params
            },
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
            percentiles: None,
            geometric_mean: None,
            warmup_runs: None,
            runs_planned: None,
            first_run: None,
            diagnostics: None,
            per_run_parameters: None,
        },
    ];

    let actual = String::from_utf8(
        exporter
            .serialize(
                &results,
                Some(&results[0]),
                Some(Unit::Second),
                SortOrder::Command,
            )
            .unwrap(),
    )
    .unwrap();

    insta::assert_snapshot!(actual, @r#"
    command,mean,stddev,median,user,system,min,max,parameter_secs,relative_speed,relative_speed_stddev
    ref_cmd,1,0,1,0.5,0.5,1,1,,1,
    param_cmd,2,0,2,1,1,2,2,2,2,
    "#);
}

#[test]
fn test_csv_heterogeneous_parameters() {
    use std::collections::BTreeMap;
    let exporter = CsvExporter::default();

    let results = vec![
        BenchmarkResult {
            command: String::from("cmd_a"),
            command_with_unused_parameters: String::from("cmd_a"),
            mean: 1.0,
            stddev: None,
            median: 1.0,
            user: 0.5,
            system: 0.5,
            cpu_percent: None,
            min: 1.0,
            max: 1.0,
            times: None,
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0)],
            parameters: {
                let mut params = BTreeMap::new();
                params.insert("alpha".into(), "val1".into());
                params
            },
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
            percentiles: None,
            geometric_mean: None,
            warmup_runs: None,
            runs_planned: None,
            first_run: None,
            diagnostics: None,
            per_run_parameters: None,
        },
        BenchmarkResult {
            command: String::from("cmd_b"),
            command_with_unused_parameters: String::from("cmd_b"),
            mean: 2.0,
            stddev: None,
            median: 2.0,
            user: 1.0,
            system: 1.0,
            cpu_percent: None,
            min: 2.0,
            max: 2.0,
            times: None,
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0)],
            parameters: {
                let mut params = BTreeMap::new();
                params.insert("beta".into(), "val2".into());
                params
            },
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
            percentiles: None,
            geometric_mean: None,
            warmup_runs: None,
            runs_planned: None,
            first_run: None,
            diagnostics: None,
            per_run_parameters: None,
        },
    ];

    let actual = String::from_utf8(
        exporter
            .serialize(&results, None, Some(Unit::Second), SortOrder::Command)
            .unwrap(),
    )
    .unwrap();

    insta::assert_snapshot!(actual, @r#"
    command,mean,stddev,median,user,system,min,max,parameter_alpha,parameter_beta,relative_speed,relative_speed_stddev
    cmd_a,1,0,1,0.5,0.5,1,1,val1,,1,
    cmd_b,2,0,2,1,1,2,2,,val2,2,
    "#);
}
