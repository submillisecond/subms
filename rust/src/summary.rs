//! Structured bench summary - the typed counterpart to the standard subms JSON.
//!
//! Build one with [`crate::summarize`] (or [`crate::summarize_lean`] for the
//! sample-free variant). Hand the summary to [`crate::print_summary`],
//! [`crate::assert_p99_under`], or [`crate::summary_to_json`].
//!
//! Field shape is byte-equivalent to Java's `SubMsBenchSummary` / `SubMsStageSummary`.

use std::collections::BTreeMap;

/// Per-stage summary. The counterpart of one `stages.<name>` entry in the
/// subms JSON contract.
///
/// `samples_ns` carries the downsampled (max-500) chronological timeline
/// emitted by [`crate::summarize`]; [`crate::summarize_lean`] leaves it `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubMsStageSummary {
    pub name: String,
    pub count: usize,
    pub p50_ns: u64,
    pub p99_ns: u64,
    pub p999_ns: u64,
    pub max_ns: u64,
    pub mean_ns: u64,
    pub samples_ns: Option<Vec<u64>>,
}

/// Typed summary of one bench run. Mirrors the on-disk JSON downstream tooling persists per workload.
///
/// Stage order is registration order, matching the harness.
#[derive(Clone, Debug)]
pub struct SubMsBenchSummary {
    pub workload: String,
    pub lang: String,
    pub timestamp: String,
    pub inputs: BTreeMap<String, String>,
    pub meta: BTreeMap<String, String>,
    pub stages: Vec<SubMsStageSummary>,
}

impl SubMsBenchSummary {
    /// Look up a stage by name.
    pub fn stage(&self, name: &str) -> Option<&SubMsStageSummary> {
        self.stages.iter().find(|s| s.name == name)
    }
}

/// A sweep is a collection of [`SubMsBenchSummary`] runs that share a
/// workload but vary one or more input parameters - typically `entries` for
/// scale-curve studies, or `bloom_mode` for a feature-toggle comparison.
///
/// `varied_input_key` names the input that differs across runs; `None` means
/// runs are labelled by ordinal.
///
/// Java counterpart: `com.submillisecond.perf.SubMsBenchSweep`.
#[derive(Clone, Debug)]
pub struct SubMsBenchSweep {
    pub workload: String,
    pub lang: String,
    pub varied_input_key: Option<String>,
    pub runs: Vec<SubMsBenchSummary>,
}

/// One row in a per-stage diff: a baseline value, a candidate value, and the
/// deltas between them.
///
/// `delta_pct` is signed: positive means the candidate is slower (regression
/// for latency metrics). Java counterpart: `SubMsMetricDiff`.
#[derive(Clone, Debug, PartialEq)]
pub struct SubMsMetricDiff {
    pub metric: String,
    pub baseline_ns: u64,
    pub candidate_ns: u64,
    pub delta_ns: i64,
    /// `100 * delta_ns / baseline_ns`. `f64::INFINITY` when baseline = 0.
    pub delta_pct: f64,
}

/// Per-stage diff: every percentile + mean compared between two runs of the
/// same stage. `worst_regression_pct` is the most-positive `delta_pct` across
/// all metrics - the headline number a CI gate keys off.
///
/// Java counterpart: `SubMsStageDiff`.
#[derive(Clone, Debug)]
pub struct SubMsStageDiff {
    pub stage: String,
    pub metrics: Vec<SubMsMetricDiff>,
    pub worst_regression_pct: f64,
}

/// Typed diff between a baseline and a candidate [`SubMsBenchSummary`].
/// Java counterpart: `SubMsBenchDiff`.
#[derive(Clone, Debug)]
pub struct SubMsBenchDiff {
    pub baseline_workload: String,
    pub candidate_workload: String,
    pub lang: String,
    pub stages: Vec<SubMsStageDiff>,
    pub baseline_only_stages: Vec<String>,
    pub candidate_only_stages: Vec<String>,
    pub regression_threshold_pct: f64,
}

impl SubMsBenchDiff {
    /// `true` if any stage's `worst_regression_pct` exceeded
    /// `regression_threshold_pct`.
    pub fn has_regression(&self) -> bool {
        self.stages
            .iter()
            .any(|s| s.worst_regression_pct > self.regression_threshold_pct)
    }

    /// The worst-regressing stage, or `None` if all stages stayed within the threshold.
    pub fn worst_stage(&self) -> Option<&SubMsStageDiff> {
        self.stages.iter().max_by(|a, b| {
            a.worst_regression_pct
                .partial_cmp(&b.worst_regression_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}
