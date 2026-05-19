//! Shared bench helpers. Pipeline:
//!
//! ```text
//! recipe -> SubMsPerfHarness -> SubMsBenchSummary -> { print, assert, JSON }
//! ```
//!
//! [`summarize`] turns the raw harness into a typed [`SubMsBenchSummary`].
//! [`print_summary`], [`assert_p99_under`], and [`summary_to_json`] are
//! presenters / asserters on top of that data; none of them recompute stats.
//!
//! The Java sibling ships the same surface (`SubMsBench.summarize`,
//! `SubMsBench.printSummary`, `SubMsBench.summaryToJson`) with byte-equivalent
//! output, so tooling can consume either runtime interchangeably.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, Write};

use crate::{
    SubMsBenchDiff, SubMsBenchParams, SubMsBenchSummary, SubMsBenchSweep, SubMsMetricDiff,
    SubMsPerfHarness, SubMsRecipe, SubMsStageDiff, SubMsStageSummary,
};

// ---------------------------------------------------------------------
// Summarise (structured)
// ---------------------------------------------------------------------

/// Build a [`SubMsBenchSummary`] from the harness. Includes the downsampled
/// (max-500) chronological per-stage timeline. Order matches stage
/// registration.
pub fn summarize(h: &SubMsPerfHarness) -> SubMsBenchSummary {
    summarize_internal(h, /*include_samples*/ true)
}

/// Same as [`summarize`] but drops the per-stage sample arrays. Use when you
/// only need count + percentiles + mean.
pub fn summarize_lean(h: &SubMsPerfHarness) -> SubMsBenchSummary {
    summarize_internal(h, /*include_samples*/ false)
}

fn summarize_internal(h: &SubMsPerfHarness, include_samples: bool) -> SubMsBenchSummary {
    let stages = h
        .stages()
        .iter()
        .map(|s| summarize_stage(s.name(), s.samples(), include_samples))
        .collect();
    SubMsBenchSummary {
        workload: h.workload().to_string(),
        lang: h.lang().to_string(),
        timestamp: h.timestamp(),
        inputs: clone_map(h.inputs()),
        meta: clone_map(h.meta()),
        stages,
    }
}

fn summarize_stage(name: &str, chronological: &[u64], include_samples: bool) -> SubMsStageSummary {
    let mut sorted = chronological.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    let p50 = percentile(&sorted, 0.50);
    let p99 = percentile(&sorted, 0.99);
    let p999 = percentile(&sorted, 0.999);
    let max = sorted.last().copied().unwrap_or(0);
    let mean = if n == 0 { 0 } else { sorted.iter().sum::<u64>() / n as u64 };
    let samples_ns = if include_samples { Some(downsample(chronological)) } else { None };
    SubMsStageSummary {
        name: name.to_string(),
        count: n,
        p50_ns: p50,
        p99_ns: p99,
        p999_ns: p999,
        max_ns: max,
        mean_ns: mean,
        samples_ns,
    }
}

/// Evenly-spaced downsample to at most 500 points, chronological order preserved.
pub(crate) fn downsample(chronological: &[u64]) -> Vec<u64> {
    let n = chronological.len();
    if n == 0 {
        return Vec::new();
    }
    let step = (n / 500).max(1);
    chronological.iter().copied().step_by(step).collect()
}

fn clone_map(src: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    src.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

// ---------------------------------------------------------------------
// Print (presenter)
// ---------------------------------------------------------------------

/// Print a fixed-width percentile table for every stage in the summary, in
/// registration order. Byte-equivalent to Java's `SubMsBench.printSummary`.
pub fn print_summary<W: Write>(s: &SubMsBenchSummary, out: &mut W) -> io::Result<()> {
    writeln!(
        out,
        "  {:<9}  {:>9}  {:>9}  {:>9}  {:>9}  {:>9}",
        "stage", "p50", "p99", "p99.9", "max", "mean"
    )?;
    for stage in &s.stages {
        writeln!(
            out,
            "  {:<9}  {:>9}  {:>9}  {:>9}  {:>9}  {:>9}",
            stage.name,
            format_ns(stage.p50_ns),
            format_ns(stage.p99_ns),
            format_ns(stage.p999_ns),
            format_ns(stage.max_ns),
            format_ns(stage.mean_ns),
        )?;
    }
    Ok(())
}

/// Compact unit-aware ns formatter. Sub-microsecond stays in ns; sub-millisecond
/// goes to us with one decimal; everything else is ms to two decimals. Matches
/// Java's `SubMsBench.formatNs`.
pub fn format_ns(ns: u64) -> String {
    if ns < 1_000 {
        format!("{}ns", ns)
    } else if ns < 1_000_000 {
        format!("{:.1}us", ns as f64 / 1_000.0)
    } else {
        format!("{:.2}ms", ns as f64 / 1_000_000.0)
    }
}

// ---------------------------------------------------------------------
// Assert
// ---------------------------------------------------------------------

/// A single stage-level p99 assertion.
#[derive(Debug, Clone, Copy)]
pub struct SubMsBenchAssertion {
    /// Stage name as registered with the harness.
    pub stage: &'static str,
    /// Upper bound, ns.
    pub p99_ns_max: u64,
}

/// Accept either a [`SubMsBenchSummary`] (recommended) or a
/// [`SubMsPerfHarness`] (back-compat). Used by [`assert_p99_under`].
pub trait SubMsAssertionTarget {
    fn lookup_p99_ns(&self, stage: &str) -> Option<u64>;
}

impl SubMsAssertionTarget for SubMsBenchSummary {
    fn lookup_p99_ns(&self, stage: &str) -> Option<u64> {
        self.stage(stage).map(|s| s.p99_ns)
    }
}

impl SubMsAssertionTarget for SubMsPerfHarness {
    fn lookup_p99_ns(&self, stage: &str) -> Option<u64> {
        let st = self.stage_by_name(stage)?;
        let mut sorted = st.samples().to_vec();
        sorted.sort_unstable();
        Some(percentile(&sorted, 0.99))
    }
}

/// `Err` on the first stage that exceeds its p99 bound (or is missing).
/// Accepts either a summary or the raw harness.
pub fn assert_p99_under<T: SubMsAssertionTarget + ?Sized>(
    target: &T,
    assertions: &[SubMsBenchAssertion],
) -> Result<(), String> {
    for a in assertions {
        let p99 = target
            .lookup_p99_ns(a.stage)
            .ok_or_else(|| format!("stage '{}' not found", a.stage))?;
        if p99 > a.p99_ns_max {
            return Err(format!(
                "stage '{}' p99 = {} ns exceeded limit {} ns",
                a.stage, p99, a.p99_ns_max
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------

/// Alias of [`crate::recipe::benchmark`]; matches Java's `Bench.runBench`.
pub fn run_bench<R: SubMsRecipe + ?Sized>(recipe: &R, params: &SubMsBenchParams) -> SubMsPerfHarness {
    crate::recipe::benchmark(recipe, params)
}

// ---------------------------------------------------------------------
// JSON (presenter)
// ---------------------------------------------------------------------

/// Serialise the summary to the standard subms JSON shape. Byte-equivalent
/// to Java's `SubMsBench.summaryToJson`.
pub fn summary_to_json<W: Write>(s: &SubMsBenchSummary, out: &mut W) -> io::Result<()> {
    let mut buf = String::with_capacity(64 * 1024);
    append_summary_json(&mut buf, s);
    out.write_all(buf.as_bytes())?;
    out.write_all(b"\n")?;
    Ok(())
}

pub(crate) fn append_summary_json(out: &mut String, s: &SubMsBenchSummary) {
    out.push('{');
    json_kv_str(out, "workload", &s.workload);
    out.push(',');
    json_kv_str(out, "lang", &s.lang);
    out.push(',');
    json_kv_str(out, "timestamp", &s.timestamp);
    out.push(',');
    out.push_str("\"inputs\":");
    json_map(out, &s.inputs);
    out.push(',');
    out.push_str("\"meta\":");
    json_map(out, &s.meta);
    out.push(',');
    out.push_str("\"stages\":{");
    for (i, stage) in s.stages.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json_str(out, &stage.name);
        out.push(':');
        stage_json(out, stage);
    }
    out.push_str("}}");
}

fn stage_json(out: &mut String, stage: &SubMsStageSummary) {
    out.push('{');
    let _ = write!(out, "\"count\":{},", stage.count);
    let _ = write!(out, "\"p50_ns\":{},", stage.p50_ns);
    let _ = write!(out, "\"p99_ns\":{},", stage.p99_ns);
    let _ = write!(out, "\"p999_ns\":{},", stage.p999_ns);
    let _ = write!(out, "\"max_ns\":{},", stage.max_ns);
    let _ = write!(out, "\"mean_ns\":{},", stage.mean_ns);
    out.push_str("\"samples_ns\":[");
    if let Some(samples) = &stage.samples_ns {
        for (i, x) in samples.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "{}", x);
        }
    }
    out.push_str("]}");
}

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn json_kv_str(out: &mut String, k: &str, v: &str) {
    json_str(out, k);
    out.push(':');
    json_str(out, v);
}

fn json_map(out: &mut String, m: &BTreeMap<String, String>) {
    out.push('{');
    for (i, (k, v)) in m.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json_kv_str(out, k, v);
    }
    out.push('}');
}

// ---------------------------------------------------------------------
// Sweep (multi-run varied-input pipeline)
// ---------------------------------------------------------------------

/// Run the recipe once per element of `params_list`, summarise each run, and
/// bundle the summaries into a [`SubMsBenchSweep`]. `varied_input_key` should
/// name the input that differs across runs (typically `"entries"`); pass
/// `None` to leave it unset.
pub fn run_sweep<R: SubMsRecipe + ?Sized>(
    recipe: &R,
    params_list: &[SubMsBenchParams],
    varied_input_key: Option<&str>,
) -> SubMsBenchSweep {
    let runs = params_list
        .iter()
        .map(|p| summarize(&run_bench(recipe, p)))
        .collect();
    SubMsBenchSweep {
        workload: recipe.name().to_string(),
        lang: "rust".to_string(),
        varied_input_key: varied_input_key.map(|s| s.to_string()),
        runs,
    }
}

/// Bundle pre-computed summaries (e.g. captured separately) into a sweep. All
/// summaries should share a workload; the first summary's workload is used.
pub fn summarize_sweep(
    summaries: Vec<SubMsBenchSummary>,
    varied_input_key: Option<&str>,
) -> SubMsBenchSweep {
    assert!(!summaries.is_empty(), "summarize_sweep requires at least one run");
    SubMsBenchSweep {
        workload: summaries[0].workload.clone(),
        lang: summaries[0].lang.clone(),
        varied_input_key: varied_input_key.map(|s| s.to_string()),
        runs: summaries,
    }
}

/// Print a pivoted percentile table per stage: one block per stage, one row
/// per run, labelled by the varied input value (or by ordinal if no varied
/// key was supplied). Byte-equivalent to Java's `SubMsBench.printSweep`.
pub fn print_sweep<W: Write>(sweep: &SubMsBenchSweep, out: &mut W) -> io::Result<()> {
    if sweep.runs.is_empty() {
        writeln!(out, "(empty sweep)")?;
        return Ok(());
    }
    let first = &sweep.runs[0];
    let header_label = sweep.varied_input_key.as_deref().unwrap_or("run");
    for stage in &first.stages {
        writeln!(out, "stage: {}", stage.name)?;
        writeln!(
            out,
            "  {:<15}  {:>9}  {:>9}  {:>9}  {:>9}  {:>9}  {:>9}",
            header_label, "count", "p50", "p99", "p99.9", "max", "mean"
        )?;
        for (i, run) in sweep.runs.iter().enumerate() {
            let label = match &sweep.varied_input_key {
                Some(k) => run.inputs.get(k).cloned().unwrap_or_else(|| "?".to_string()),
                None => format!("run {}", i + 1),
            };
            match run.stage(&stage.name) {
                None => writeln!(out, "  {:<15}  (stage missing)", label)?,
                Some(s) => writeln!(
                    out,
                    "  {:<15}  {:>9}  {:>9}  {:>9}  {:>9}  {:>9}  {:>9}",
                    label,
                    s.count,
                    format_ns(s.p50_ns),
                    format_ns(s.p99_ns),
                    format_ns(s.p999_ns),
                    format_ns(s.max_ns),
                    format_ns(s.mean_ns)
                )?,
            }
        }
        writeln!(out)?;
    }
    Ok(())
}

/// Emit a JSON array of run-summaries, identical shape to
/// on-disk `perf/<lang>.json`. Byte-equivalent to Java's
/// `SubMsBench.sweepToJson`.
pub fn sweep_to_json<W: Write>(sweep: &SubMsBenchSweep, out: &mut W) -> io::Result<()> {
    let mut buf = String::with_capacity(64 * 1024);
    buf.push('[');
    for (i, run) in sweep.runs.iter().enumerate() {
        if i > 0 {
            buf.push(',');
        }
        append_summary_json(&mut buf, run);
    }
    buf.push(']');
    out.write_all(buf.as_bytes())?;
    out.write_all(b"\n")?;
    Ok(())
}

// ---------------------------------------------------------------------
// Diff (baseline vs candidate regression detection)
// ---------------------------------------------------------------------

/// Default percent above which a stage's metric is considered a regression for
/// the subms-perf-gate CI workflow. Matches Java's
/// `SubMsBench.DEFAULT_REGRESSION_THRESHOLD_PCT`.
pub const DEFAULT_REGRESSION_THRESHOLD_PCT: f64 = 10.0;

/// Build a typed diff between two summaries using the default 10 % threshold.
pub fn diff_summary(
    baseline: &SubMsBenchSummary,
    candidate: &SubMsBenchSummary,
) -> SubMsBenchDiff {
    diff_summary_with(baseline, candidate, DEFAULT_REGRESSION_THRESHOLD_PCT)
}

/// Same as [`diff_summary`] but caller specifies the regression threshold.
pub fn diff_summary_with(
    baseline: &SubMsBenchSummary,
    candidate: &SubMsBenchSummary,
    regression_threshold_pct: f64,
) -> SubMsBenchDiff {
    let baseline_names: Vec<&str> = baseline.stages.iter().map(|s| s.name.as_str()).collect();
    let candidate_names: std::collections::BTreeSet<&str> =
        candidate.stages.iter().map(|s| s.name.as_str()).collect();

    let mut stage_diffs = Vec::new();
    for cand in &candidate.stages {
        if let Some(base) = baseline.stage(&cand.name) {
            stage_diffs.push(diff_stage(base, cand));
        }
    }

    let candidate_name_set: std::collections::BTreeSet<&str> = candidate_names.clone();
    let baseline_name_set: std::collections::BTreeSet<&str> =
        baseline_names.iter().copied().collect();
    let baseline_only: Vec<String> = baseline_names
        .iter()
        .filter(|n| !candidate_name_set.contains(*n))
        .map(|s| s.to_string())
        .collect();
    let candidate_only: Vec<String> = candidate
        .stages
        .iter()
        .map(|s| s.name.clone())
        .filter(|n| !baseline_name_set.contains(n.as_str()))
        .collect();

    SubMsBenchDiff {
        baseline_workload: baseline.workload.clone(),
        candidate_workload: candidate.workload.clone(),
        lang: candidate.lang.clone(),
        stages: stage_diffs,
        baseline_only_stages: baseline_only,
        candidate_only_stages: candidate_only,
        regression_threshold_pct,
    }
}

fn diff_stage(baseline: &SubMsStageSummary, candidate: &SubMsStageSummary) -> SubMsStageDiff {
    let metrics = vec![
        metric_diff("p50", baseline.p50_ns, candidate.p50_ns),
        metric_diff("p99", baseline.p99_ns, candidate.p99_ns),
        metric_diff("p99.9", baseline.p999_ns, candidate.p999_ns),
        metric_diff("max", baseline.max_ns, candidate.max_ns),
        metric_diff("mean", baseline.mean_ns, candidate.mean_ns),
    ];
    let worst = metrics
        .iter()
        .filter(|m| m.delta_pct.is_finite())
        .map(|m| m.delta_pct)
        .fold(0.0_f64, f64::max);
    SubMsStageDiff {
        stage: baseline.name.clone(),
        metrics,
        worst_regression_pct: worst,
    }
}

fn metric_diff(name: &str, baseline: u64, candidate: u64) -> SubMsMetricDiff {
    let delta_ns = candidate as i64 - baseline as i64;
    let delta_pct = if baseline == 0 {
        if candidate == 0 { 0.0 } else { f64::INFINITY }
    } else {
        (100.0 * delta_ns as f64) / baseline as f64
    };
    SubMsMetricDiff {
        metric: name.to_string(),
        baseline_ns: baseline,
        candidate_ns: candidate,
        delta_ns,
        delta_pct,
    }
}

/// Print a regression-table view of the diff. Byte-equivalent to Java's
/// `SubMsBench.printDiff`.
pub fn print_diff<W: Write>(diff: &SubMsBenchDiff, out: &mut W) -> io::Result<()> {
    writeln!(
        out,
        "diff: {} vs {} ({})  threshold=+{:.1}%",
        diff.baseline_workload, diff.candidate_workload, diff.lang, diff.regression_threshold_pct
    )?;
    writeln!(
        out,
        "  {:<12}  {:<7}  {:>9}  {:>9}  {:>9}  {:>9}  {}",
        "stage", "metric", "baseline", "candidate", "delta", "%delta", "verdict"
    )?;
    for stage in &diff.stages {
        for m in &stage.metrics {
            let pct_str = if m.delta_pct.is_finite() {
                format!("{:+.1}%", m.delta_pct)
            } else {
                "+inf%".to_string()
            };
            let verdict = if m.delta_pct.is_finite() && m.delta_pct > diff.regression_threshold_pct {
                "REGRESSED"
            } else {
                "ok"
            };
            let abs = m.delta_ns.unsigned_abs();
            let delta_str = if m.delta_ns >= 0 {
                format!("+{}", format_ns(abs))
            } else {
                format!("-{}", format_ns(abs))
            };
            writeln!(
                out,
                "  {:<12}  {:<7}  {:>9}  {:>9}  {:>9}  {:>9}  {}",
                stage.stage,
                m.metric,
                format_ns(m.baseline_ns),
                format_ns(m.candidate_ns),
                delta_str,
                pct_str,
                verdict,
            )?;
        }
    }
    if !diff.baseline_only_stages.is_empty() {
        writeln!(out, "  stages only in baseline:  {}", diff.baseline_only_stages.join(", "))?;
    }
    if !diff.candidate_only_stages.is_empty() {
        writeln!(out, "  stages only in candidate: {}", diff.candidate_only_stages.join(", "))?;
    }
    Ok(())
}

/// Emit the diff as a single JSON object for downstream CI tooling.
/// Byte-equivalent to Java's `SubMsBench.diffToJson`.
pub fn diff_to_json<W: Write>(diff: &SubMsBenchDiff, out: &mut W) -> io::Result<()> {
    let mut buf = String::with_capacity(8 * 1024);
    append_diff_json(&mut buf, diff);
    out.write_all(buf.as_bytes())?;
    out.write_all(b"\n")?;
    Ok(())
}

fn append_diff_json(out: &mut String, diff: &SubMsBenchDiff) {
    out.push('{');
    json_kv_str(out, "baseline_workload", &diff.baseline_workload);
    out.push(',');
    json_kv_str(out, "candidate_workload", &diff.candidate_workload);
    out.push(',');
    json_kv_str(out, "lang", &diff.lang);
    out.push(',');
    let _ = write!(out, "\"regression_threshold_pct\":{},", diff.regression_threshold_pct);
    let _ = write!(out, "\"has_regression\":{},", diff.has_regression());
    out.push_str("\"stages\":[");
    for (i, s) in diff.stages.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        json_kv_str(out, "stage", &s.stage);
        out.push(',');
        let _ = write!(out, "\"worst_regression_pct\":{},", json_number(s.worst_regression_pct));
        out.push_str("\"metrics\":[");
        for (j, m) in s.metrics.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push('{');
            json_kv_str(out, "metric", &m.metric);
            out.push(',');
            let _ = write!(out, "\"baseline_ns\":{},", m.baseline_ns);
            let _ = write!(out, "\"candidate_ns\":{},", m.candidate_ns);
            let _ = write!(out, "\"delta_ns\":{},", m.delta_ns);
            let _ = write!(out, "\"delta_pct\":{}", json_number(m.delta_pct));
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push_str("],");
    out.push_str("\"baseline_only_stages\":[");
    for (i, n) in diff.baseline_only_stages.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json_str(out, n);
    }
    out.push_str("],");
    out.push_str("\"candidate_only_stages\":[");
    for (i, n) in diff.candidate_only_stages.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json_str(out, n);
    }
    out.push_str("]}");
}

/// Render a finite f64 as a JSON number; non-finite values become `null`
/// (JSON has no inf/NaN literal).
fn json_number(d: f64) -> String {
    if d.is_finite() { d.to_string() } else { "null".to_string() }
}

// ---------------------------------------------------------------------
// Numeric helpers
// ---------------------------------------------------------------------

/// Percentile over a sorted ns slice. Empty -> 0. Index is `floor(q*n).min(n-1)`
/// so q=1.0 is the max.
pub fn percentile(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((q * sorted.len() as f64) as usize).min(sorted.len() - 1);
    sorted[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_empty_is_zero() {
        assert_eq!(percentile(&[], 0.5), 0);
    }

    #[test]
    fn percentile_single_value() {
        assert_eq!(percentile(&[42], 0.0), 42);
        assert_eq!(percentile(&[42], 0.5), 42);
        assert_eq!(percentile(&[42], 1.0), 42);
    }

    #[test]
    fn percentile_known_distribution() {
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&v, 0.50), 51);
        assert_eq!(percentile(&v, 0.99), 100);
        assert_eq!(percentile(&v, 0.999), 100);
        assert_eq!(percentile(&v, 1.0), 100);
    }

    struct FixedSubMsRecipe;
    impl SubMsRecipe for FixedSubMsRecipe {
        fn name(&self) -> &str { "fixed-recipe" }
        fn run(&self, h: &mut SubMsPerfHarness, _params: &SubMsBenchParams) {
            let s = h.stage("step", 4);
            s.record(100);
            s.record(200);
            s.record(300);
            s.record(400);
        }
    }

    #[test]
    fn run_bench_drives_recipe_through_harness() {
        let p = SubMsBenchParams::default();
        let h = run_bench(&FixedSubMsRecipe, &p);
        let stage = h.stage_by_name("step").expect("step recorded");
        assert_eq!(stage.samples().len(), 4);
    }

    #[test]
    fn summarize_populates_percentiles_and_samples() {
        let p = SubMsBenchParams::default();
        let h = run_bench(&FixedSubMsRecipe, &p);
        let s = summarize(&h);
        assert_eq!(s.stages.len(), 1);
        let st = &s.stages[0];
        assert_eq!(st.name, "step");
        assert_eq!(st.count, 4);
        assert_eq!(st.p50_ns, 300);
        assert_eq!(st.p99_ns, 400);
        assert_eq!(st.max_ns, 400);
        assert_eq!(st.mean_ns, 250);
        assert_eq!(st.samples_ns.as_ref().unwrap().len(), 4);
    }

    #[test]
    fn summarize_lean_drops_samples() {
        let p = SubMsBenchParams::default();
        let h = run_bench(&FixedSubMsRecipe, &p);
        let s = summarize_lean(&h);
        assert!(s.stages[0].samples_ns.is_none());
    }

    #[test]
    fn assert_p99_under_passes_when_below_limit() {
        let p = SubMsBenchParams::default();
        let h = run_bench(&FixedSubMsRecipe, &p);
        let s = summarize_lean(&h);
        assert_p99_under(
            &s,
            &[SubMsBenchAssertion { stage: "step", p99_ns_max: 400 }],
        )
        .expect("p99=400 should satisfy max=400");
    }

    #[test]
    fn assert_p99_under_errors_when_above_limit() {
        let p = SubMsBenchParams::default();
        let h = run_bench(&FixedSubMsRecipe, &p);
        let s = summarize_lean(&h);
        let err = assert_p99_under(
            &s,
            &[SubMsBenchAssertion { stage: "step", p99_ns_max: 399 }],
        )
        .unwrap_err();
        assert!(err.contains("step"));
        assert!(err.contains("400"));
        assert!(err.contains("399"));
    }

    #[test]
    fn assert_p99_under_errors_when_stage_missing() {
        let p = SubMsBenchParams::default();
        let h = run_bench(&FixedSubMsRecipe, &p);
        let s = summarize_lean(&h);
        let err = assert_p99_under(
            &s,
            &[SubMsBenchAssertion { stage: "ghost", p99_ns_max: 1 }],
        )
        .unwrap_err();
        assert!(err.contains("ghost"));
        assert!(err.contains("not found"));
    }

    #[test]
    fn format_ns_uses_three_unit_tiers() {
        assert_eq!(format_ns(0), "0ns");
        assert_eq!(format_ns(999), "999ns");
        assert_eq!(format_ns(1_000), "1.0us");
        assert_eq!(format_ns(36_000), "36.0us");
        assert_eq!(format_ns(999_999), "1000.0us");
        assert_eq!(format_ns(1_000_000), "1.00ms");
        assert_eq!(format_ns(3_590_000), "3.59ms");
    }

    #[test]
    fn run_sweep_runs_recipe_once_per_params_set() {
        let params = vec![
            SubMsBenchParams { entries: 4, warmup: 0, seed: 0 },
            SubMsBenchParams { entries: 4, warmup: 0, seed: 1 },
        ];
        let sweep = run_sweep(&FixedSubMsRecipe, &params, Some("seed"));
        assert_eq!(sweep.runs.len(), 2);
        assert_eq!(sweep.varied_input_key.as_deref(), Some("seed"));
        // Both runs hit the fixed recipe's "step" stage.
        assert_eq!(sweep.runs[0].stages[0].name, "step");
        assert_eq!(sweep.runs[1].stages[0].name, "step");
    }

    #[test]
    fn summarize_sweep_bundles_existing_summaries() {
        let p = SubMsBenchParams::default();
        let a = summarize(&run_bench(&FixedSubMsRecipe, &p));
        let b = summarize(&run_bench(&FixedSubMsRecipe, &p));
        let sweep = summarize_sweep(vec![a, b], Some("entries"));
        assert_eq!(sweep.runs.len(), 2);
        assert_eq!(sweep.workload, "fixed-recipe");
        assert_eq!(sweep.lang, "rust");
    }

    #[test]
    fn print_sweep_pivots_by_stage_and_labels_rows() {
        let params = vec![
            SubMsBenchParams { entries: 4, warmup: 0, seed: 0 },
            SubMsBenchParams { entries: 4, warmup: 0, seed: 0 },
        ];
        let sweep = run_sweep(&FixedSubMsRecipe, &params, None);
        let mut buf = Vec::new();
        print_sweep(&sweep, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("stage: step"));
        assert!(out.contains("run 1"));
        assert!(out.contains("run 2"));
    }

    #[test]
    fn sweep_to_json_emits_array() {
        let params = vec![
            SubMsBenchParams { entries: 4, warmup: 0, seed: 0 },
            SubMsBenchParams { entries: 4, warmup: 0, seed: 0 },
        ];
        let sweep = run_sweep(&FixedSubMsRecipe, &params, Some("seed"));
        let mut buf = Vec::new();
        sweep_to_json(&sweep, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.starts_with('['));
        assert!(out.contains("\"workload\":\"fixed-recipe\""));
        assert!(out.contains("\"stages\":{"));
    }

    // ---- diff tests -----------------------------------------------------

    struct ExplicitRecipe { values: Vec<u64> }
    impl SubMsRecipe for ExplicitRecipe {
        fn name(&self) -> &str { "explicit" }
        fn run(&self, h: &mut SubMsPerfHarness, _p: &SubMsBenchParams) {
            let s = h.stage("put", self.values.len());
            for v in &self.values { s.record(*v); }
        }
    }

    #[test]
    fn diff_summary_computes_per_metric_deltas() {
        let p = SubMsBenchParams::default();
        let base = summarize_lean(&run_bench(&ExplicitRecipe { values: vec![100, 200, 300, 400] }, &p));
        let cand = summarize_lean(&run_bench(&ExplicitRecipe { values: vec![110, 220, 330, 440] }, &p));
        let d = diff_summary(&base, &cand);
        assert_eq!(d.stages.len(), 1);
        let put = &d.stages[0];
        for m in &put.metrics {
            assert!((m.delta_pct - 10.0).abs() < 1e-9, "{}: {}", m.metric, m.delta_pct);
        }
        assert!((put.worst_regression_pct - 10.0).abs() < 1e-9);
    }

    #[test]
    fn diff_summary_flags_regression_above_threshold() {
        let p = SubMsBenchParams::default();
        let base = summarize_lean(&run_bench(&ExplicitRecipe { values: vec![100, 200, 300, 400] }, &p));
        let cand = summarize_lean(&run_bench(&ExplicitRecipe { values: vec![200, 400, 600, 800] }, &p));
        let d = diff_summary_with(&base, &cand, 50.0);
        assert!(d.has_regression());
        assert_eq!(d.worst_stage().unwrap().stage, "put");
    }

    #[test]
    fn diff_summary_does_not_flag_when_all_improved() {
        let p = SubMsBenchParams::default();
        let base = summarize_lean(&run_bench(&ExplicitRecipe { values: vec![200, 400, 600, 800] }, &p));
        let cand = summarize_lean(&run_bench(&ExplicitRecipe { values: vec![100, 200, 300, 400] }, &p));
        let d = diff_summary_with(&base, &cand, 10.0);
        assert!(!d.has_regression());
    }

    #[test]
    fn print_diff_emits_table_with_verdict_column() {
        let p = SubMsBenchParams::default();
        let base = summarize_lean(&run_bench(&ExplicitRecipe { values: vec![100, 200, 300, 400] }, &p));
        let cand = summarize_lean(&run_bench(&ExplicitRecipe { values: vec![200, 400, 600, 800] }, &p));
        let d = diff_summary_with(&base, &cand, 50.0);
        let mut buf = Vec::new();
        print_diff(&d, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("stage"));
        assert!(out.contains("verdict"));
        assert!(out.contains("REGRESSED"));
    }

    #[test]
    fn diff_to_json_emits_expected_keys() {
        let p = SubMsBenchParams::default();
        let base = summarize_lean(&run_bench(&ExplicitRecipe { values: vec![100, 200, 300, 400] }, &p));
        let cand = summarize_lean(&run_bench(&ExplicitRecipe { values: vec![110, 220, 330, 440] }, &p));
        let d = diff_summary_with(&base, &cand, 5.0);
        let mut buf = Vec::new();
        diff_to_json(&d, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("\"has_regression\":true"));
        assert!(out.contains("\"stages\":["));
        assert!(out.contains("\"metric\":\"p99\""));
        assert!(out.contains("\"delta_pct\""));
    }

    #[test]
    fn print_summary_produces_aligned_table() {
        let p = SubMsBenchParams::default();
        let h = run_bench(&FixedSubMsRecipe, &p);
        let s = summarize_lean(&h);
        let mut buf = Vec::new();
        print_summary(&s, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("stage"));
        assert!(out.contains("p99"));
        assert!(out.contains("step"));
    }
}
