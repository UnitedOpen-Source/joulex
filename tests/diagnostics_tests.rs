//! Tests for the interference diagnostics: trend, multimodality and
//! outlier-inflated variance warnings, and the `diagnostics` JSON block.

mod common;
use common::hyperfine;

use predicates::prelude::*;

#[test]
fn stable_benchmarks_have_no_diagnostic_warnings() {
    hyperfine()
        .args(["--debug-mode", "--runs=40", "sleep 0.1"])
        .assert()
        .success()
        .stderr(predicate::str::contains("trend").not())
        .stderr(predicate::str::contains("multimodal").not())
        .stderr(predicate::str::contains("variance").not());
}

#[cfg(unix)]
mod unix {
    use super::*;

    /// Sleeps 10 ms + 1 ms per run: a clear upward trend
    const RAMP: &str = "sleep $(printf '0.%03d' $((10 + JOULEX_ITERATION)))";
    /// Alternates between 10 ms and 40 ms: two separated groups
    const TWO_GROUPS: &str =
        "if [ $((JOULEX_ITERATION % 2)) = 0 ]; then sleep 0.01; else sleep 0.04; fi";
    /// 10 ms, with a 300 ms run twice in 100 runs. System noise can add a few
    /// small outliers (the MAD of 10 ms sleeps is tiny), so the count isn't
    /// asserted, and 100 runs keep all outliers well below the 10% at which
    /// they would count as a cluster instead.
    const RARE_OUTLIERS: &str =
        "if [ $((JOULEX_ITERATION % 50)) = 5 ]; then sleep 0.3; else sleep 0.01; fi";

    #[test]
    fn a_ramp_triggers_the_trend_warning_and_is_exported() {
        let dir = tempfile::tempdir().unwrap();
        let json = dir.path().join("out.json");
        hyperfine()
            .args(["--runs=30", "--export-json"])
            .arg(&json)
            .arg(RAMP)
            .assert()
            .success()
            .stderr(predicate::str::contains("systematic upward trend"));

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
        let diagnostics = &json["results"][0]["diagnostics"];
        assert!(diagnostics["trend_rel"].as_f64().unwrap() > 0.5);
        assert!(diagnostics["trend_p"].as_f64().unwrap() < 0.001);
    }

    #[test]
    fn two_groups_trigger_the_multimodal_warning() {
        hyperfine()
            .args(["--runs=40", TWO_GROUPS])
            .assert()
            .success()
            .stderr(predicate::str::contains("looks multimodal"))
            // The slower group is a mode, not "outliers"
            .stderr(predicate::str::contains("of the variance").not());
    }

    #[test]
    fn rare_huge_outliers_trigger_the_inflated_variance_warning() {
        hyperfine()
            .args(["--runs=100", RARE_OUTLIERS])
            .assert()
            .success()
            .stderr(
                predicate::str::is_match(r"\d+% of the variance is caused by \d+ outliers")
                    .unwrap(),
            )
            // Replaces the generic outlier warning
            .stderr(predicate::str::contains("Statistical outliers were detected").not());
    }

    #[test]
    fn diagnostic_warnings_can_be_suppressed() {
        hyperfine()
            .args(["--runs=30", "--suppress-outlier-warnings", RAMP])
            .assert()
            .success()
            .stderr(predicate::str::contains("trend").not());
    }
}
