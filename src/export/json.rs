use serde::*;
use serde_json::to_vec_pretty;

use super::metadata::ExportMetadata;
use super::Exporter;
use crate::benchmark::benchmark_result::BenchmarkResult;
use crate::benchmark::relative_speed;
use crate::metric::Metric;
use crate::options::SortOrder;
use crate::util::units::Unit;

use anyhow::Result;

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct PrimaryMetricStats {
    pub metric: Metric,
    pub mean: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stddev: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub samples: Option<Vec<f64>>,
}

#[derive(Serialize, Debug)]
struct JoulexSummary<'a> {
    metric: Metric,
    /// Information about the joulex run (version, command line, date, labels, system)
    #[serde(skip_serializing_if = "Option::is_none")]
    joulex: Option<&'a ExportMetadata>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    interrupted: bool,
    results: Vec<ResultEntry<'a>>,
}

#[derive(Serialize, Debug)]
struct ResultEntry<'a> {
    #[serde(flatten)]
    result: &'a BenchmarkResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_metric: Option<PrimaryMetricStats>,
    /// Mean relative to the fastest result (1.0 = fastest), as in the Markdown export.
    /// Omitted if it cannot be computed (e.g. a mean of zero).
    #[serde(skip_serializing_if = "Option::is_none")]
    relative_speed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    relative_speed_stddev: Option<f64>,
}

#[derive(Default)]
pub struct JsonExporter {
    pub metadata: Option<ExportMetadata>,
}

impl Exporter for JsonExporter {
    fn serialize(
        &self,
        results: &[BenchmarkResult],
        reference: Option<&BenchmarkResult>,
        _unit: Option<Unit>,
        _sort_order: SortOrder,
        metric: Metric,
    ) -> Result<Vec<u8>> {
        let relative = if results.is_empty() {
            None
        } else {
            let baseline = reference.unwrap_or_else(|| relative_speed::best_of(results, metric));
            relative_speed::compute_with_check_from_reference(
                results,
                baseline,
                SortOrder::Command,
                metric,
            )
        };
        let results = results
            .iter()
            .enumerate()
            .map(|(i, result)| {
                let entry = relative.as_ref().map(|r| &r[i]);
                let primary_metric = if metric != Metric::Wall {
                    Some(PrimaryMetricStats {
                        metric,
                        mean: result.primary_mean(metric),
                        stddev: result.primary_stddev(metric),
                        samples: result.primary_samples(metric),
                    })
                } else {
                    None
                };
                ResultEntry {
                    result,
                    primary_metric,
                    relative_speed: entry.map(|e| e.relative_speed).filter(|r| r.is_finite()),
                    // the baseline has no spread relative to itself
                    relative_speed_stddev: entry
                        .filter(|e| !e.is_reference)
                        .and_then(|e| e.relative_speed_stddev),
                }
            })
            .collect();

        let mut output = to_vec_pretty(&JoulexSummary {
            metric,
            joulex: self.metadata.as_ref(),
            interrupted: crate::util::interrupt::interrupted(),
            results,
        });
        if let Ok(ref mut content) = output {
            content.push(b'\n');
        }

        Ok(output?)
    }
}
