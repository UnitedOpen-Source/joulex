use anyhow::{Context, Result};
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

    #[test]
    fn test_import_json_missing_file() {
        let err = import_json("non_existent_json_file.json");
        assert!(err.is_err());
    }
}
