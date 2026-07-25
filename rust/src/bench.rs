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
    SubMsPerfHarness, SubMsRecipe, SubMsStageDiff, SubMsStageSummary, stats,
};

// ---------------------------------------------------------------------
// Summarise (structured)
// ---------------------------------------------------------------------

/// Build a [`SubMsBenchSummary`] from the harness. Includes the downsampled
/// chronological per-stage timeline, capped at the harness's
/// [`SubMsPerfHarness::sample_cap`] (default 500). Order matches stage
/// registration.
///
/// If the harness has a [`crate::SubMsObserver`] registered, fires
/// `on_summarize` exactly once with the produced summary before returning.
/// Other summary variants (`summarize_lean`, `summarize_skipping`,
/// `summarize_windowed`) do NOT fire the observer - they're considered
/// internal re-summarisations rather than the canonical post-bench result.
pub fn summarize(h: &SubMsPerfHarness) -> SubMsBenchSummary {
    let summary = summarize_internal(
        h,
        /*include_samples*/ true,
        /*skip_warmup*/ 0,
        h.sample_cap(),
    );
    if let Some(obs) = h.observer() {
        obs.on_summarize(&summary);
    }
    summary
}

/// Same as [`summarize`] but drops the per-stage sample arrays. Use when you
/// only need count + percentiles + mean.
pub fn summarize_lean(h: &SubMsPerfHarness) -> SubMsBenchSummary {
    summarize_internal(
        h,
        /*include_samples*/ false,
        /*skip_warmup*/ 0,
        h.sample_cap(),
    )
}

/// Same as [`summarize`] but discards the first `skip_warmup` samples per
/// stage before computing percentiles + mean + stddev. Use when the
/// recipe can't insert a pre-pass warmup itself (e.g. a JIT- or cache-
/// cold first ~1k operations would skew p99).
///
/// `samples_ns` in the output reflects the trimmed timeline.
pub fn summarize_skipping(h: &SubMsPerfHarness, skip_warmup: usize) -> SubMsBenchSummary {
    summarize_internal(
        h,
        /*include_samples*/ true,
        skip_warmup,
        h.sample_cap(),
    )
}

/// Slice each stage's chronological sample buffer into `window` equal-sized
/// chunks and produce a [`SubMsBenchSummary`] per chunk. Useful for
/// rolling-window p99 analysis - "how did p99 evolve across the run?"
///
/// Windows are sample-count-based, not wall-clock-based, since the harness
/// doesn't record per-sample timestamps. For wall-clock windows, ensure
/// the workload runs at a roughly steady rate; then the i-th window
/// approximates the i-th time slice.
///
/// Returns an empty vector if the harness has zero stages.
pub fn summarize_windowed(h: &SubMsPerfHarness, window: usize) -> Vec<SubMsBenchSummary> {
    let window = window.max(1);
    // Find the longest stage; that determines how many windows we emit.
    let max_len = h
        .stages()
        .iter()
        .map(|s| s.samples().len())
        .max()
        .unwrap_or(0);
    if max_len == 0 {
        return Vec::new();
    }
    let n_windows = max_len.div_ceil(window);
    let mut out = Vec::with_capacity(n_windows);
    for w in 0..n_windows {
        let start = w * window;
        let stages = h
            .stages()
            .iter()
            .map(|s| {
                let samples = s.samples();
                let end = (start + window).min(samples.len());
                let slice = if start < samples.len() {
                    &samples[start..end]
                } else {
                    &[][..]
                };
                summarize_stage(
                    s.name(),
                    slice,
                    /*include_samples*/ false,
                    /*sample_cap*/ 500,
                )
            })
            .collect();
        out.push(SubMsBenchSummary {
            workload: h.workload().to_string(),
            lang: h.lang().to_string(),
            timestamp: h.timestamp(),
            cpu_core: None,
            cpu_affinity: None,
            inputs: {
                let mut m = clone_map(h.inputs());
                m.insert("__window_index".to_string(), w.to_string());
                m.insert("__window_size".to_string(), window.to_string());
                m
            },
            meta: clone_map(h.meta()),
            stages,
        });
    }
    out
}

// percentile_sweep moved to `crate::stats::percentile_sweep`. Recipes
// previously importing it from this module can keep using `subms::percentile_sweep`
// via the top-level re-export.

fn summarize_internal(
    h: &SubMsPerfHarness,
    include_samples: bool,
    skip_warmup: usize,
    sample_cap: usize,
) -> SubMsBenchSummary {
    let stages = h
        .stages()
        .iter()
        .map(|s| {
            let trimmed = if skip_warmup > 0 && s.samples().len() > skip_warmup {
                &s.samples()[skip_warmup..]
            } else {
                s.samples()
            };
            summarize_stage(s.name(), trimmed, include_samples, sample_cap)
        })
        .collect();
    let (cpu_core, cpu_affinity) = cpu_placement();
    SubMsBenchSummary {
        workload: h.workload().to_string(),
        lang: h.lang().to_string(),
        timestamp: h.timestamp(),
        cpu_core,
        cpu_affinity,
        inputs: clone_map(h.inputs()),
        meta: clone_map(h.meta()),
        stages,
    }
}

/// Best-effort per-run CPU placement from Linux `/proc`. Returns
/// (last-run core, allowed-affinity list). `(None, None)` off Linux or on any
/// read/parse failure - the harness never fails a bench over provenance.
fn cpu_placement() -> (Option<u32>, Option<String>) {
    let core = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|s| {
            // Fields after the final ')' (which closes `comm`) begin at field 3, so
            // field 39 (`processor`, the last core the task ran on) is index 36.
            let start = s.rfind(')').map(|i| i + 1)?;
            s[start..]
                .split_whitespace()
                .nth(36)
                .and_then(|v| v.parse::<u32>().ok())
        });
    let affinity = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
                .map(|v| v.trim().to_string())
        });
    (core, affinity)
}

fn summarize_stage(
    name: &str,
    chronological: &[u64],
    include_samples: bool,
    sample_cap: usize,
) -> SubMsStageSummary {
    let mut sorted = chronological.to_vec();
    sorted.sort_unstable();
    let samples_ns = if include_samples {
        Some(downsample(chronological, sample_cap))
    } else {
        None
    };
    SubMsStageSummary {
        name: name.to_string(),
        count: sorted.len(),
        p50_ns: stats::percentile(&sorted, 0.50),
        p99_ns: stats::percentile(&sorted, 0.99),
        p999_ns: stats::percentile(&sorted, 0.999),
        max_ns: sorted.last().copied().unwrap_or(0),
        mean_ns: stats::mean(chronological),
        stddev_ns: stats::stddev(chronological),
        cdf_buckets_ns: stats::cdf_buckets(chronological),
        jitter_score: stats::jitter_score(chronological),
        samples_ns,
    }
}

/// Evenly-spaced downsample to at most `cap` points, chronological order
/// preserved. `cap == 0` is treated as 1. Pass a `cap >= len` (e.g. equal to
/// `entries`) to keep every point.
pub(crate) fn downsample(chronological: &[u64], cap: usize) -> Vec<u64> {
    let n = chronological.len();
    if n == 0 {
        return Vec::new();
    }
    let step = (n / cap.max(1)).max(1);
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
        Some(stats::percentile(&sorted, 0.99))
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
pub fn run_bench<R: SubMsRecipe + ?Sized>(
    recipe: &R,
    params: &SubMsBenchParams,
) -> SubMsPerfHarness {
    crate::recipe::benchmark(recipe, params)
}

/// Run a contended workload `iterations_per_thread` times on `threads`
/// threads and discard the timings. The standard way to warm up
/// recipes whose hot path runs under multi-producer contention: serial
/// warmup compiles the function under uncontended cache-line traffic,
/// which doesn't expose the JIT / branch predictor to the actual
/// contended pattern the timed loop uses. Without this pre-pass the
/// first 1-2k samples in the timed loop run "cold" under contention
/// and inflate p99 by 3-5x.
///
/// `work` is invoked once per iteration on each thread with the
/// `(thread_id, iteration)` pair.
pub fn contended_warmup<F>(threads: usize, iterations_per_thread: usize, work: F)
where
    F: Fn(usize, usize) + Send + Sync + 'static + Copy,
{
    let mut handles = Vec::with_capacity(threads);
    for tid in 0..threads {
        handles.push(std::thread::spawn(move || {
            for i in 0..iterations_per_thread {
                work(tid, i);
            }
        }));
    }
    for h in handles {
        h.join().expect("contended_warmup thread");
    }
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
    out.push_str("\"cpu\":");
    cpu_json(out, s.cpu_core, s.cpu_affinity.as_deref());
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

fn cpu_json(out: &mut String, core: Option<u32>, affinity: Option<&str>) {
    if core.is_none() && affinity.is_none() {
        out.push_str("null");
        return;
    }
    out.push('{');
    match core {
        Some(c) => {
            let _ = write!(out, "\"core\":{c}");
        }
        None => out.push_str("\"core\":null"),
    }
    out.push_str(",\"affinity\":");
    match affinity {
        Some(a) => json_str(out, a),
        None => out.push_str("null"),
    }
    out.push('}');
}

fn stage_json(out: &mut String, stage: &SubMsStageSummary) {
    out.push('{');
    let _ = write!(out, "\"count\":{},", stage.count);
    let _ = write!(out, "\"p50_ns\":{},", stage.p50_ns);
    let _ = write!(out, "\"p99_ns\":{},", stage.p99_ns);
    let _ = write!(out, "\"p999_ns\":{},", stage.p999_ns);
    let _ = write!(out, "\"max_ns\":{},", stage.max_ns);
    let _ = write!(out, "\"mean_ns\":{},", stage.mean_ns);
    let _ = write!(out, "\"stddev_ns\":{},", stage.stddev_ns);
    let _ = write!(out, "\"jitter_score\":{:.4},", stage.jitter_score);
    out.push_str("\"cdf_buckets_ns\":[");
    for (i, c) in stage.cdf_buckets_ns.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{}", c);
    }
    out.push_str("],");
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
    assert!(
        !summaries.is_empty(),
        "summarize_sweep requires at least one run"
    );
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
                Some(k) => run
                    .inputs
                    .get(k)
                    .cloned()
                    .unwrap_or_else(|| "?".to_string()),
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
pub fn diff_summary(baseline: &SubMsBenchSummary, candidate: &SubMsBenchSummary) -> SubMsBenchDiff {
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
        "  {:<12}  {:<7}  {:>9}  {:>9}  {:>9}  {:>9}  verdict",
        "stage", "metric", "baseline", "candidate", "delta", "%delta"
    )?;
    for stage in &diff.stages {
        for m in &stage.metrics {
            let pct_str = if m.delta_pct.is_finite() {
                format!("{:+.1}%", m.delta_pct)
            } else {
                "+inf%".to_string()
            };
            let verdict = if m.delta_pct.is_finite() && m.delta_pct > diff.regression_threshold_pct
            {
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
        writeln!(
            out,
            "  stages only in baseline:  {}",
            diff.baseline_only_stages.join(", ")
        )?;
    }
    if !diff.candidate_only_stages.is_empty() {
        writeln!(
            out,
            "  stages only in candidate: {}",
            diff.candidate_only_stages.join(", ")
        )?;
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
    let _ = write!(
        out,
        "\"regression_threshold_pct\":{},",
        diff.regression_threshold_pct
    );
    let _ = write!(out, "\"has_regression\":{},", diff.has_regression());
    out.push_str("\"stages\":[");
    for (i, s) in diff.stages.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        json_kv_str(out, "stage", &s.stage);
        out.push(',');
        let _ = write!(
            out,
            "\"worst_regression_pct\":{},",
            json_number(s.worst_regression_pct)
        );
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
    if d.is_finite() {
        d.to_string()
    } else {
        "null".to_string()
    }
}

// percentile is INTERNAL ONLY. Recipes don't reach for it directly;
// they read the percentile fields off `SubMsStageSummary`. External
// consumers wanting a standalone percentile fn pull in `subms-stats`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_empty_is_zero() {
        assert_eq!(stats::percentile(&[], 0.5), 0);
    }

    #[test]
    fn percentile_single_value() {
        assert_eq!(stats::percentile(&[42], 0.0), 42);
        assert_eq!(stats::percentile(&[42], 0.5), 42);
        assert_eq!(stats::percentile(&[42], 1.0), 42);
    }

    #[test]
    fn percentile_known_distribution() {
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!(stats::percentile(&v, 0.50), 51);
        assert_eq!(stats::percentile(&v, 0.99), 100);
        assert_eq!(stats::percentile(&v, 0.999), 100);
        assert_eq!(stats::percentile(&v, 1.0), 100);
    }

    struct FixedSubMsRecipe;
    impl SubMsRecipe for FixedSubMsRecipe {
        fn name(&self) -> &str {
            "fixed-recipe"
        }
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
    fn cpu_json_emits_object_or_null() {
        let mut a = String::new();
        cpu_json(&mut a, Some(1), Some("1"));
        assert_eq!(a, r#"{"core":1,"affinity":"1"}"#);
        let mut b = String::new();
        cpu_json(&mut b, Some(0), Some("0-1"));
        assert_eq!(b, r#"{"core":0,"affinity":"0-1"}"#);
        let mut c = String::new();
        cpu_json(&mut c, None, None);
        assert_eq!(c, "null");
    }

    #[test]
    fn downsample_respects_cap() {
        let full: Vec<u64> = (0..2000).collect();
        // cap below len → thinned to ~cap points (step = 2000/500 = 4).
        assert_eq!(downsample(&full, 500).len(), 500);
        // cap >= len → every point kept.
        assert_eq!(downsample(&full, 5000).len(), 2000);
        assert_eq!(downsample(&full, 2000).len(), 2000);
        // cap 0 is treated as 1 (keep a single stride, not a divide-by-zero).
        assert_eq!(downsample(&full, 0).len(), 1);
        // empty in, empty out.
        assert!(downsample(&[], 500).is_empty());
    }

    #[test]
    fn benchmark_threads_sample_cap_onto_harness() {
        let p = SubMsBenchParams {
            entries: 4,
            warmup: 0,
            seed: 0,
            sample_cap: 12_345,
        };
        let h = crate::benchmark(&FixedSubMsRecipe, &p);
        assert_eq!(h.sample_cap(), 12_345);
        // Default params keep the 500 back-compat cap.
        let hd = crate::benchmark(&FixedSubMsRecipe, &SubMsBenchParams::default());
        assert_eq!(hd.sample_cap(), 500);
    }

    #[test]
    fn assert_p99_under_passes_when_below_limit() {
        let p = SubMsBenchParams::default();
        let h = run_bench(&FixedSubMsRecipe, &p);
        let s = summarize_lean(&h);
        assert_p99_under(
            &s,
            &[SubMsBenchAssertion {
                stage: "step",
                p99_ns_max: 400,
            }],
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
            &[SubMsBenchAssertion {
                stage: "step",
                p99_ns_max: 399,
            }],
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
            &[SubMsBenchAssertion {
                stage: "ghost",
                p99_ns_max: 1,
            }],
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
            SubMsBenchParams {
                entries: 4,
                warmup: 0,
                seed: 0,
                sample_cap: 500,
            },
            SubMsBenchParams {
                entries: 4,
                warmup: 0,
                seed: 1,
                sample_cap: 500,
            },
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
            SubMsBenchParams {
                entries: 4,
                warmup: 0,
                seed: 0,
                sample_cap: 500,
            },
            SubMsBenchParams {
                entries: 4,
                warmup: 0,
                seed: 0,
                sample_cap: 500,
            },
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
            SubMsBenchParams {
                entries: 4,
                warmup: 0,
                seed: 0,
                sample_cap: 500,
            },
            SubMsBenchParams {
                entries: 4,
                warmup: 0,
                seed: 0,
                sample_cap: 500,
            },
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

    struct ExplicitRecipe {
        values: Vec<u64>,
    }
    impl SubMsRecipe for ExplicitRecipe {
        fn name(&self) -> &str {
            "explicit"
        }
        fn run(&self, h: &mut SubMsPerfHarness, _p: &SubMsBenchParams) {
            let s = h.stage("put", self.values.len());
            for v in &self.values {
                s.record(*v);
            }
        }
    }

    #[test]
    fn diff_summary_computes_per_metric_deltas() {
        let p = SubMsBenchParams::default();
        let base = summarize_lean(&run_bench(
            &ExplicitRecipe {
                values: vec![100, 200, 300, 400],
            },
            &p,
        ));
        let cand = summarize_lean(&run_bench(
            &ExplicitRecipe {
                values: vec![110, 220, 330, 440],
            },
            &p,
        ));
        let d = diff_summary(&base, &cand);
        assert_eq!(d.stages.len(), 1);
        let put = &d.stages[0];
        for m in &put.metrics {
            assert!(
                (m.delta_pct - 10.0).abs() < 1e-9,
                "{}: {}",
                m.metric,
                m.delta_pct
            );
        }
        assert!((put.worst_regression_pct - 10.0).abs() < 1e-9);
    }

    #[test]
    fn diff_summary_flags_regression_above_threshold() {
        let p = SubMsBenchParams::default();
        let base = summarize_lean(&run_bench(
            &ExplicitRecipe {
                values: vec![100, 200, 300, 400],
            },
            &p,
        ));
        let cand = summarize_lean(&run_bench(
            &ExplicitRecipe {
                values: vec![200, 400, 600, 800],
            },
            &p,
        ));
        let d = diff_summary_with(&base, &cand, 50.0);
        assert!(d.has_regression());
        assert_eq!(d.worst_stage().unwrap().stage, "put");
    }

    #[test]
    fn diff_summary_does_not_flag_when_all_improved() {
        let p = SubMsBenchParams::default();
        let base = summarize_lean(&run_bench(
            &ExplicitRecipe {
                values: vec![200, 400, 600, 800],
            },
            &p,
        ));
        let cand = summarize_lean(&run_bench(
            &ExplicitRecipe {
                values: vec![100, 200, 300, 400],
            },
            &p,
        ));
        let d = diff_summary_with(&base, &cand, 10.0);
        assert!(!d.has_regression());
    }

    #[test]
    fn print_diff_emits_table_with_verdict_column() {
        let p = SubMsBenchParams::default();
        let base = summarize_lean(&run_bench(
            &ExplicitRecipe {
                values: vec![100, 200, 300, 400],
            },
            &p,
        ));
        let cand = summarize_lean(&run_bench(
            &ExplicitRecipe {
                values: vec![200, 400, 600, 800],
            },
            &p,
        ));
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
        let base = summarize_lean(&run_bench(
            &ExplicitRecipe {
                values: vec![100, 200, 300, 400],
            },
            &p,
        ));
        let cand = summarize_lean(&run_bench(
            &ExplicitRecipe {
                values: vec![110, 220, 330, 440],
            },
            &p,
        ));
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

    // ------------------------------------------------------------
    // 0.5.0 additions: summarize_windowed, cdf_buckets_ns/jitter_score
    // JSON round-trip
    // ------------------------------------------------------------

    /// Recipe that records 100 samples into one stage so summarize_windowed
    /// has enough data for multiple buckets.
    struct ManySamplesRecipe;
    impl SubMsRecipe for ManySamplesRecipe {
        fn name(&self) -> &str {
            "many-samples"
        }
        fn run(&self, h: &mut SubMsPerfHarness, _params: &SubMsBenchParams) {
            let s = h.stage("op", 100);
            for i in 0..100 {
                // Linearly increasing values - first window will have lower
                // p99 than later windows, so summarize_windowed should
                // show monotonic non-decreasing p99 across windows.
                s.record((i + 1) * 100);
            }
        }
    }

    #[test]
    fn summarize_windowed_splits_into_correct_number_of_buckets() {
        let p = SubMsBenchParams::default();
        let h = run_bench(&ManySamplesRecipe, &p);
        let windows = summarize_windowed(&h, 20);
        // 100 samples / 20-per-window = 5 windows
        assert_eq!(windows.len(), 5);
        // Each window records the bucket index in inputs.
        for (i, w) in windows.iter().enumerate() {
            assert_eq!(
                w.inputs.get("__window_index").map(String::as_str),
                Some(i.to_string().as_str())
            );
            assert_eq!(
                w.inputs.get("__window_size").map(String::as_str),
                Some("20")
            );
        }
    }

    #[test]
    fn summarize_windowed_p99_monotonic_for_monotonic_input() {
        let p = SubMsBenchParams::default();
        let h = run_bench(&ManySamplesRecipe, &p);
        let windows = summarize_windowed(&h, 25);
        let p99s: Vec<u64> = windows.iter().map(|w| w.stages[0].p99_ns).collect();
        // Monotonic non-decreasing p99 since the recipe records
        // i*100 increasing values across the run.
        for pair in p99s.windows(2) {
            assert!(
                pair[1] >= pair[0],
                "p99 not monotonic: {} -> {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn summarize_windowed_empty_harness_returns_empty_vec() {
        // A harness with no stages.
        let h = SubMsPerfHarness::new("empty", "rust");
        let windows = summarize_windowed(&h, 10);
        assert!(windows.is_empty());
    }

    #[test]
    fn summarize_windowed_zero_window_treated_as_one() {
        let p = SubMsBenchParams::default();
        let h = run_bench(&ManySamplesRecipe, &p);
        // Window size 0 should be normalised to 1; expect 100 windows.
        let windows = summarize_windowed(&h, 0);
        assert_eq!(windows.len(), 100);
    }

    #[test]
    fn summary_to_json_emits_cdf_buckets_ns_field() {
        let p = SubMsBenchParams::default();
        let s = summarize(&run_bench(&FixedSubMsRecipe, &p));
        let mut buf = Vec::new();
        summary_to_json(&s, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(
            out.contains("\"cdf_buckets_ns\":["),
            "summary JSON must include cdf_buckets_ns: {}",
            out
        );
    }

    #[test]
    fn summary_to_json_emits_jitter_score_field() {
        let p = SubMsBenchParams::default();
        let s = summarize(&run_bench(&FixedSubMsRecipe, &p));
        let mut buf = Vec::new();
        summary_to_json(&s, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(
            out.contains("\"jitter_score\":"),
            "summary JSON must include jitter_score: {}",
            out
        );
    }

    #[test]
    fn summary_to_json_emits_stddev_ns_field() {
        let p = SubMsBenchParams::default();
        let s = summarize(&run_bench(&FixedSubMsRecipe, &p));
        let mut buf = Vec::new();
        summary_to_json(&s, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(
            out.contains("\"stddev_ns\":"),
            "summary JSON must include stddev_ns: {}",
            out
        );
    }

    #[test]
    fn summarize_populates_cdf_buckets_64_long() {
        let p = SubMsBenchParams::default();
        let s = summarize(&run_bench(&FixedSubMsRecipe, &p));
        assert_eq!(s.stages[0].cdf_buckets_ns.len(), 64);
        // Total bucket count should equal the sample count.
        let total: u64 = s.stages[0].cdf_buckets_ns.iter().sum();
        assert_eq!(total as usize, s.stages[0].count);
    }

    #[test]
    fn summarize_populates_jitter_score_in_unit_interval() {
        let p = SubMsBenchParams::default();
        let s = summarize(&run_bench(&FixedSubMsRecipe, &p));
        let jit = s.stages[0].jitter_score;
        assert!(
            (0.0..=1.0).contains(&jit),
            "jitter_score out of [0, 1]: {}",
            jit
        );
    }
}
