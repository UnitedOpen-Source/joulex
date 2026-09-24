use super::Exporter;
use crate::benchmark::benchmark_result::BenchmarkResult;
use crate::export::asciidoc::AsciidocExporter;
use crate::export::orgmode::OrgmodeExporter;
use crate::util::units::Unit;
use crate::{export::markdown::MarkdownExporter, options::SortOrder};
use std::collections::BTreeMap;

fn get_output<E: Exporter + Default>(
    results: &[BenchmarkResult],
    unit: Option<Unit>,
    sort_order: SortOrder,
) -> String {
    let exporter = E::default();
    String::from_utf8(exporter.serialize(results, unit, sort_order).unwrap()).unwrap()
}

/// Ensure the makrup output includes the table header and the multiple
/// benchmark results as a table. The list of actual times is not included
/// in the output.
///
/// This also demonstrates that the first entry's units (ms) are used to set
/// the units for all entries when the time unit is not specified.
#[test]
fn test_markup_export_auto_ms() {
    let results = [
        BenchmarkResult {
            command: String::from("sleep 0.1"),
            command_with_unused_parameters: String::from("sleep 0.1"),
            mean: 0.1057,
            stddev: Some(0.0016),
            median: 0.1057,
            user: 0.0009,
            system: 0.0011,
            cpu_percent: None,
            min: 0.1023,
            max: 0.1080,
            times: Some(vec![0.1, 0.1, 0.1]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: BTreeMap::new(),
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
        },
        BenchmarkResult {
            command: String::from("sleep 2"),
            command_with_unused_parameters: String::from("sleep 2"),
            mean: 2.0050,
            stddev: Some(0.0020),
            median: 2.0050,
            user: 0.0009,
            system: 0.0012,
            cpu_percent: None,
            min: 2.0020,
            max: 2.0080,
            times: Some(vec![2.0, 2.0, 2.0]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: BTreeMap::new(),
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
        },
    ];

    insta::assert_snapshot!(get_output::<MarkdownExporter>(&results, None, SortOrder::Command), @r#"
    | Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
    |:---|---:|---:|---:|---:|
    | `sleep 0.1` | 105.7 ± 1.6 | 102.3 | 108.0 | 1.00 |
    | `sleep 2` | 2005.0 ± 2.0 | 2002.0 | 2008.0 | 18.97 ± 0.29 |
    "#);

    insta::assert_snapshot!(get_output::<AsciidocExporter>(&results, None, SortOrder::Command), @r#"
    [cols="<,>,>,>,>"]
    |===
    | Command 
    | Mean [ms] 
    | Min [ms] 
    | Max [ms] 
    | Relative 

    | `sleep 0.1` 
    | 105.7 ± 1.6 
    | 102.3 
    | 108.0 
    | 1.00 

    | `sleep 2` 
    | 2005.0 ± 2.0 
    | 2002.0 
    | 2008.0 
    | 18.97 ± 0.29 
    |===
    "#);

    insta::assert_snapshot!(get_output::<OrgmodeExporter>(&results, None, SortOrder::Command), @r#"
    | Command  |  Mean [ms] |  Min [ms] |  Max [ms] |  Relative |
    |--+--+--+--+--|
    | =sleep 0.1=  |  105.7 ± 1.6 |  102.3 |  108.0 |  1.00 |
    | =sleep 2=  |  2005.0 ± 2.0 |  2002.0 |  2008.0 |  18.97 ± 0.29 |
    "#);
}

/// This (again) demonstrates that the first entry's units (s) are used to set
/// the units for all entries when the time unit is not given.
#[test]
fn test_markup_export_auto_s() {
    let results = [
        BenchmarkResult {
            command: String::from("sleep 2"),
            command_with_unused_parameters: String::from("sleep 2"),
            mean: 2.0050,
            stddev: Some(0.0020),
            median: 2.0050,
            user: 0.0009,
            system: 0.0012,
            cpu_percent: None,
            min: 2.0020,
            max: 2.0080,
            times: Some(vec![2.0, 2.0, 2.0]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: BTreeMap::new(),
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
        },
        BenchmarkResult {
            command: String::from("sleep 0.1"),
            command_with_unused_parameters: String::from("sleep 0.1"),
            mean: 0.1057,
            stddev: Some(0.0016),
            median: 0.1057,
            user: 0.0009,
            system: 0.0011,
            cpu_percent: None,
            min: 0.1023,
            max: 0.1080,
            times: Some(vec![0.1, 0.1, 0.1]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: BTreeMap::new(),
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
        },
    ];

    insta::assert_snapshot!(get_output::<MarkdownExporter>(&results, None, SortOrder::Command), @r#"
    | Command | Mean [s] | Min [s] | Max [s] | Relative |
    |:---|---:|---:|---:|---:|
    | `sleep 2` | 2.005 ± 0.002 | 2.002 | 2.008 | 18.97 ± 0.29 |
    | `sleep 0.1` | 0.106 ± 0.002 | 0.102 | 0.108 | 1.00 |
    "#);

    insta::assert_snapshot!(get_output::<AsciidocExporter>(&results, None, SortOrder::Command), @r#"
    [cols="<,>,>,>,>"]
    |===
    | Command 
    | Mean [s] 
    | Min [s] 
    | Max [s] 
    | Relative 

    | `sleep 2` 
    | 2.005 ± 0.002 
    | 2.002 
    | 2.008 
    | 18.97 ± 0.29 

    | `sleep 0.1` 
    | 0.106 ± 0.002 
    | 0.102 
    | 0.108 
    | 1.00 
    |===
    "#);

    insta::assert_snapshot!(get_output::<OrgmodeExporter>(&results, None, SortOrder::Command), @r#"
    | Command  |  Mean [s] |  Min [s] |  Max [s] |  Relative |
    |--+--+--+--+--|
    | =sleep 2=  |  2.005 ± 0.002 |  2.002 |  2.008 |  18.97 ± 0.29 |
    | =sleep 0.1=  |  0.106 ± 0.002 |  0.102 |  0.108 |  1.00 |
    "#);
}

/// This (again) demonstrates that the given time unit (ms) is used to set
/// the units for all entries.
#[test]
fn test_markup_export_manual_ms() {
    let timing_results = [
        BenchmarkResult {
            command: String::from("sleep 2"),
            command_with_unused_parameters: String::from("sleep 2"),
            mean: 2.0050,
            stddev: Some(0.0020),
            median: 2.0050,
            user: 0.0009,
            system: 0.0012,
            cpu_percent: None,
            min: 2.0020,
            max: 2.0080,
            times: Some(vec![2.0, 2.0, 2.0]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: BTreeMap::new(),
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
        },
        BenchmarkResult {
            command: String::from("sleep 0.1"),
            command_with_unused_parameters: String::from("sleep 0.1"),
            mean: 0.1057,
            stddev: Some(0.0016),
            median: 0.1057,
            user: 0.0009,
            system: 0.0011,
            cpu_percent: None,
            min: 0.1023,
            max: 0.1080,
            times: Some(vec![0.1, 0.1, 0.1]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: BTreeMap::new(),
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
        },
    ];

    insta::assert_snapshot!(get_output::<MarkdownExporter>(&timing_results, Some(Unit::MilliSecond), SortOrder::Command), @r#"
    | Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
    |:---|---:|---:|---:|---:|
    | `sleep 2` | 2005.0 ± 2.0 | 2002.0 | 2008.0 | 18.97 ± 0.29 |
    | `sleep 0.1` | 105.7 ± 1.6 | 102.3 | 108.0 | 1.00 |
    "#);

    insta::assert_snapshot!(get_output::<AsciidocExporter>(&timing_results, Some(Unit::MilliSecond), SortOrder::Command), @r#"
    [cols="<,>,>,>,>"]
    |===
    | Command 
    | Mean [ms] 
    | Min [ms] 
    | Max [ms] 
    | Relative 

    | `sleep 2` 
    | 2005.0 ± 2.0 
    | 2002.0 
    | 2008.0 
    | 18.97 ± 0.29 

    | `sleep 0.1` 
    | 105.7 ± 1.6 
    | 102.3 
    | 108.0 
    | 1.00 
    |===
    "#);

    insta::assert_snapshot!(get_output::<OrgmodeExporter>(&timing_results, Some(Unit::MilliSecond), SortOrder::Command), @r#"
    | Command  |  Mean [ms] |  Min [ms] |  Max [ms] |  Relative |
    |--+--+--+--+--|
    | =sleep 2=  |  2005.0 ± 2.0 |  2002.0 |  2008.0 |  18.97 ± 0.29 |
    | =sleep 0.1=  |  105.7 ± 1.6 |  102.3 |  108.0 |  1.00 |
    "#);
}

/// The given time unit (s) is used to set the units for all entries.
#[test]
fn test_markup_export_manual_s() {
    let results = [
        BenchmarkResult {
            command: String::from("sleep 2"),
            command_with_unused_parameters: String::from("sleep 2"),
            mean: 2.0050,
            stddev: Some(0.0020),
            median: 2.0050,
            user: 0.0009,
            system: 0.0012,
            cpu_percent: None,
            min: 2.0020,
            max: 2.0080,
            times: Some(vec![2.0, 2.0, 2.0]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: BTreeMap::new(),
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
        },
        BenchmarkResult {
            command: String::from("sleep 0.1"),
            command_with_unused_parameters: String::from("sleep 0.1"),
            mean: 0.1057,
            stddev: Some(0.0016),
            median: 0.1057,
            user: 0.0009,
            system: 0.0011,
            cpu_percent: None,
            min: 0.1023,
            max: 0.1080,
            times: Some(vec![0.1, 0.1, 0.1]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: BTreeMap::new(),
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
        },
    ];

    insta::assert_snapshot!(get_output::<MarkdownExporter>(&results, Some(Unit::Second), SortOrder::Command), @r#"
        | Command | Mean [s] | Min [s] | Max [s] | Relative |
        |:---|---:|---:|---:|---:|
        | `sleep 2` | 2.005 ± 0.002 | 2.002 | 2.008 | 18.97 ± 0.29 |
        | `sleep 0.1` | 0.106 ± 0.002 | 0.102 | 0.108 | 1.00 |
        "#);

    insta::assert_snapshot!(get_output::<MarkdownExporter>(&results, Some(Unit::Second), SortOrder::MeanTime), @r#"
        | Command | Mean [s] | Min [s] | Max [s] | Relative |
        |:---|---:|---:|---:|---:|
        | `sleep 0.1` | 0.106 ± 0.002 | 0.102 | 0.108 | 1.00 |
        | `sleep 2` | 2.005 ± 0.002 | 2.002 | 2.008 | 18.97 ± 0.29 |
        "#);

    insta::assert_snapshot!(get_output::<AsciidocExporter>(&results, Some(Unit::Second), SortOrder::Command), @r#"
    [cols="<,>,>,>,>"]
    |===
    | Command 
    | Mean [s] 
    | Min [s] 
    | Max [s] 
    | Relative 

    | `sleep 2` 
    | 2.005 ± 0.002 
    | 2.002 
    | 2.008 
    | 18.97 ± 0.29 

    | `sleep 0.1` 
    | 0.106 ± 0.002 
    | 0.102 
    | 0.108 
    | 1.00 
    |===
    "#);
}

#[test]
fn test_markup_export_zero_mean_displays_na() {
    let results = [
        BenchmarkResult {
            command: String::from("a"),
            command_with_unused_parameters: String::from("a"),
            mean: 0.0,
            stddev: Some(0.0),
            median: 0.0,
            user: 0.0,
            system: 0.0,
            cpu_percent: None,
            min: 0.0,
            max: 0.0,
            times: Some(vec![0.0, 0.0, 0.0]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: BTreeMap::new(),
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
        },
        BenchmarkResult {
            command: String::from("b"),
            command_with_unused_parameters: String::from("b"),
            mean: 0.0,
            stddev: Some(0.0),
            median: 0.0,
            user: 0.0,
            system: 0.0,
            cpu_percent: None,
            min: 0.0,
            max: 0.0,
            times: Some(vec![0.0, 0.0, 0.0]),
            user_times: None,
            system_times: None,
            memory_usage_byte: None,
            mean_energy_joules: None,
            mean_watts: None,
            energy_joules: None,
            exit_codes: vec![Some(0), Some(0), Some(0)],
            parameters: BTreeMap::new(),
            omitted_failed_runs: Vec::new(),
            discarded_outliers: Vec::new(),
            resources: None,
        },
    ];

    let md = get_output::<MarkdownExporter>(&results, None, SortOrder::Command);
    assert!(!md.contains("inf"));
    assert!(!md.contains("NaN"));
    assert!(md.contains("n/a"));

    let adoc = get_output::<AsciidocExporter>(&results, None, SortOrder::Command);
    assert!(!adoc.contains("inf"));
    assert!(!adoc.contains("NaN"));
    assert!(adoc.contains("n/a"));

    let org = get_output::<OrgmodeExporter>(&results, None, SortOrder::Command);
    assert!(!org.contains("inf"));
    assert!(!org.contains("NaN"));
    assert!(org.contains("n/a"));
}
