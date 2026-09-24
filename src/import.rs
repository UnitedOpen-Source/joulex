use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::benchmark::benchmark_result::BenchmarkResult;
use crate::util::sanitize::escape_control_chars;

#[derive(Deserialize, Debug)]
struct HyperfineSummary {
    results: Vec<BenchmarkResult>,
}

/// Import previously exported JSON benchmark results from the given file path.
pub fn import_json(path: &str) -> Result<Vec<BenchmarkResult>> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Could not open import file '{path}'"))?;
    let reader = std::io::BufReader::new(file);
    let mut summary: HyperfineSummary = serde_json::from_reader(reader)
        .with_context(|| format!("Failed to parse import JSON '{path}'"))?;
    if summary.results.is_empty() {
        bail!("Import file '{path}' contains no benchmark results");
    }
    for (index, res) in summary.results.iter().enumerate() {
        validate(res).with_context(|| {
            format!(
                "Invalid benchmark result #{} ('{}') in import file '{path}'",
                index + 1,
                crate::util::sanitize::escape_control_chars(&res.command)
            )
        })?;
    }
    for res in &mut summary.results {
        if res.command_with_unused_parameters.is_empty() {
            res.command_with_unused_parameters = res.command.clone();
        }
        // Imported files are untrusted: neutralize control characters in every
        // string that is later printed to the terminal or written to exports.
        res.command = escape_control_chars(&res.command).into_owned();
        res.command_with_unused_parameters =
            escape_control_chars(&res.command_with_unused_parameters).into_owned();
        res.parameters = std::mem::take(&mut res.parameters)
            .into_iter()
            .map(|(k, v)| {
                (
                    escape_control_chars(&k).into_owned(),
                    escape_control_chars(&v).into_owned(),
                )
            })
            .collect();
    }
    Ok(summary.results)
}

/// Check that an imported result is internally consistent, so that malformed
/// or hand-edited files cannot produce nonsensical statistics (e.g. negative
/// times or mismatched per-run arrays).
fn validate(res: &BenchmarkResult) -> Result<()> {
    let non_negative = |name: &str, value: f64| -> Result<()> {
        if !value.is_finite() || value < 0.0 {
            bail!("'{name}' must be a finite, non-negative number (got {value})");
        }
        Ok(())
    };

    non_negative("mean", res.mean)?;
    non_negative("median", res.median)?;
    non_negative("min", res.min)?;
    non_negative("max", res.max)?;
    non_negative("user", res.user)?;
    non_negative("system", res.system)?;
    if let Some(stddev) = res.stddev {
        non_negative("stddev", stddev)?;
    }
    if res.min > res.max {
        bail!("'min' ({}) is larger than 'max' ({})", res.min, res.max);
    }

    let arrays: [(&str, Option<&[f64]>); 4] = [
        ("times", res.times.as_deref()),
        ("user_times", res.user_times.as_deref()),
        ("system_times", res.system_times.as_deref()),
        ("energy_joules", res.energy_joules.as_deref()),
    ];
    for (name, values) in arrays {
        for (i, &v) in values.unwrap_or_default().iter().enumerate() {
            if !v.is_finite() || v < 0.0 {
                bail!("'{name}[{i}]' must be a finite, non-negative number (got {v})");
            }
        }
    }

    // Per-run arrays that joulex always records once per run must line up
    // with `times`. (Energy is excluded: samples can be missing.)
    if let Some(times) = res.times.as_deref().filter(|t| !t.is_empty()) {
        // `min`/`max` are computed from `times` by joulex and hyperfine alike.
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
        let t_min = times.iter().copied().fold(f64::INFINITY, f64::min);
        let t_max = times.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !close(t_min, res.min) || !close(t_max, res.max) {
            bail!(
                "'min'/'max' ({}/{}) do not match the range of 'times' ({t_min}/{t_max})",
                res.min,
                res.max
            );
        }

        let n = times.len();
        let lengths = [
            ("exit_codes", Some(res.exit_codes.len()).filter(|&l| l > 0)),
            ("user_times", res.user_times.as_ref().map(Vec::len)),
            ("system_times", res.system_times.as_ref().map(Vec::len)),
            (
                "memory_usage_byte",
                res.memory_usage_byte.as_ref().map(Vec::len),
            ),
        ];
        for (name, len) in lengths {
            if let Some(len) = len {
                if len != n {
                    bail!("'{name}' has {len} entries, but 'times' has {n}");
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_import_json_roundtrip() {
        let mut temp = NamedTempFile::new().unwrap();
        let json_content = r#"{
            "results": [
                {
                    "command": "echo test",
                    "mean": 0.05,
                    "stddev": 0.002,
                    "median": 0.049,
                    "user": 0.01,
                    "system": 0.02,
                    "min": 0.045,
                    "max": 0.055,
                    "times": [0.045, 0.055],
                    "exit_codes": [0, 0]
                }
            ]
        }"#;
        temp.write_all(json_content.as_bytes()).unwrap();

        let results = import_json(temp.path().to_str().unwrap()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "echo test");
        assert_eq!(results[0].command_with_unused_parameters, "echo test");
        assert_eq!(results[0].mean, 0.05);
        assert_eq!(results[0].exit_codes, vec![Some(0), Some(0)]);
    }

    #[test]
    fn test_import_json_escapes_control_characters() {
        let mut temp = NamedTempFile::new().unwrap();
        let json_content = r#"{"results":[{"command":"a\u001b[2Jb","mean":1,"stddev":0,
            "median":1,"user":0,"system":0,"min":1,"max":1,"times":[1],"exit_codes":[0],
            "parameters":{"k\u0007":"v\u009b"}}]}"#;
        temp.write_all(json_content.as_bytes()).unwrap();

        let results = import_json(temp.path().to_str().unwrap()).unwrap();
        assert_eq!(results[0].command, "a\\u{1b}[2Jb");
        assert_eq!(results[0].command_with_unused_parameters, "a\\u{1b}[2Jb");
        assert_eq!(
            results[0].parameters.get("k\\u{7}").map(String::as_str),
            Some("v\\u{9b}")
        );
    }

    fn import_str(json: &str) -> Result<Vec<BenchmarkResult>> {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(json.as_bytes()).unwrap();
        import_json(temp.path().to_str().unwrap())
    }

    const VALID: &str =
        r#""command":"a","mean":1,"stddev":0.1,"median":1,"user":0,"system":0,"min":0.9,"max":1.1"#;

    #[test]
    fn test_import_json_rejects_empty_results() {
        let err = import_str(r#"{"results":[]}"#).unwrap_err();
        assert!(format!("{err:#}").contains("contains no benchmark results"));
    }

    #[test]
    fn test_import_json_rejects_negative_or_non_finite_values() {
        let err = import_str(
            r#"{"results":[{"command":"a","mean":-1,"stddev":0.1,"median":1,"user":0,"system":0,"min":0,"max":1}]}"#,
        )
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("Invalid benchmark result #1 ('a')"), "{msg}");
        assert!(
            msg.contains("'mean' must be a finite, non-negative number"),
            "{msg}"
        );

        let err =
            import_str(&format!(r#"{{"results":[{{{VALID},"times":[1,-2]}}]}}"#)).unwrap_err();
        assert!(format!("{err:#}").contains("'times[1]'"));
    }

    #[test]
    fn test_import_json_rejects_min_larger_than_max() {
        let err = import_str(
            r#"{"results":[{"command":"a","mean":1,"stddev":0,"median":1,"user":0,"system":0,"min":2,"max":1}]}"#,
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("'min' (2) is larger than 'max' (1)"));
    }

    #[test]
    fn test_import_json_rejects_mismatched_array_lengths() {
        let err = import_str(&format!(
            r#"{{"results":[{{{VALID},"times":[0.9,1,1.1],"exit_codes":[0]}}]}}"#
        ))
        .unwrap_err();
        assert!(format!("{err:#}").contains("'exit_codes' has 1 entries, but 'times' has 3"));
    }

    #[test]
    fn test_import_json_rejects_times_outside_min_max() {
        let err = import_str(&format!(
            r#"{{"results":[{{{VALID},"times":[0.9,1.1,3.0]}}]}}"#
        ))
        .unwrap_err();
        assert!(format!("{err:#}").contains("do not match the range of 'times'"));
    }

    #[test]
    fn test_import_json_accepts_results_without_optional_arrays() {
        // Older exports and hand-written files may omit times/exit_codes entirely.
        let results = import_str(&format!(r#"{{"results":[{{{VALID}}}]}}"#)).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_import_json_missing_file() {
        let err = import_json("non_existent_json_file.json");
        assert!(err.is_err());
    }
}
