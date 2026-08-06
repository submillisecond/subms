//! `subms` - tiny std-only perf harness. Records timed samples per stage and
//! emits a stable JSON shape consumed by [submillisecond.com](https://submillisecond.com).
//!
//! # Pipeline
//!
//! ```text
//! recipe -> SubMsPerfHarness -> SubMsBenchSummary -> { print, assert, JSON }
//! ```
//!
//! [`summarize`] turns the raw harness into a typed [`SubMsBenchSummary`].
//! [`print_summary`], [`assert_p99_under`], and [`summary_to_json`] are
//! presenters / asserters that consume the summary - they never recompute stats.
//!
//! # Example
//!
//! ```
//! use subms::{SubMsPerfHarness, summarize, print_summary, summary_to_json};
//!
//! let mut h = SubMsPerfHarness::new("lsm-tree", "rust");
//! h.input("entries", &50_000.to_string());
//! h.input("bloom_mode", "on");
//! h.add_meta("sstables", "46");
//!
//! let put = h.stage("put", 50_000);
//! for _ in 0..50_000 {
//!     put.time(|| { /* work under test */ });
//! }
//!
//! let summary = summarize(&h);
//! print_summary(&summary, &mut std::io::stdout()).unwrap();
//! summary_to_json(&summary, &mut std::io::stdout()).unwrap();
//! ```
//!
//! # JSON shape (stable; matches the Java sibling jar)
//!
//! ```text
//! {
//!   "workload": "lsm-tree",
//!   "lang": "rust",
//!   "timestamp": "2026-05-13T20:24:38Z",
//!   "inputs":  { "<k>": "<v>", ... },
//!   "meta":    { "<k>": "<v>", ... },
//!   "stages": {
//!     "<name>": {
//!       "count": <int>,
//!       "p50_ns": <int>, "p99_ns": <int>, "p999_ns": <int>, "max_ns": <int>,
//!       "mean_ns": <int>,
//!       "samples_ns": [<int>, ...]
//!     }
//!   }
//! }
//! ```

pub mod bench;
pub mod bench_config;
pub mod bench_loops;
pub mod env;
pub mod feature;
pub mod growth;
pub mod observer;
pub mod params;
pub mod recipe;
mod stats; // private - internal to bench summary computation only
pub mod summary;
pub mod timer;
pub mod util;

pub use bench::{
    DEFAULT_REGRESSION_THRESHOLD_PCT, SubMsBenchAssertion, assert_p99_under, contended_warmup,
    diff_summary, diff_summary_with, diff_to_json, format_ns, print_diff, print_summary,
    print_sweep, run_bench, run_sweep, summarize, summarize_lean, summarize_skipping,
    summarize_sweep, summarize_windowed, summary_to_json, sweep_to_json,
};
pub use bench_config::{SubMsBenchConfig, SubMsCpuPin};
pub use bench_loops::{bench_indexed_op, bench_keyed_op, bench_templated_op};
pub use feature::{
    Json, SubMsFeatureCategory, SubMsFeatureManifest, SubMsP99Source, SubMsStageClass,
    classify_feature, parse_json, roll_up_stages,
};
pub use growth::{
    GROWTH_VERSION, SubMsGrowthClass, SubMsGrowthRecipe, SubMsGrowthReport, SubMsGrowthRound,
    SubMsGrowthVerdict, assert_growth_holds, grow, growth_to_json,
};

// NB: percentile / mean / stddev / cdf_buckets / jitter_score are NOT
// re-exported from `subms`. They're computed internally to build the
// JSON summary but the public-API surface of the bench harness should
// stay small. Anyone wanting rich stats (percentile_sweep, tail
// analysis, KS, Cohen's d, bootstrap CIs, ...) should add
// `subms-stats = "0.5"` directly. Recipes should depend on `subms`
// only and read percentiles off the SubMsStageSummary.
pub use params::{parse_bool, parse_string, parse_u64, parse_usize};
pub use recipe::{SubMsBenchParams, SubMsRecipe, benchmark};
pub use summary::{
    SubMsBenchDiff, SubMsBenchSummary, SubMsBenchSweep, SubMsMetricDiff, SubMsStageDiff,
    SubMsStageSummary,
};
// SubMsPacedStage is defined inline in this file - re-export it from the root.
pub use env::{SubMsAppEnv, SubMsAppRegion, env_bool, env_f64, env_i64, env_or, env_str, env_u64};
pub use observer::{ObservationCtx, SubMsObserver, SubMsStageKind};
pub use timer::{SubMsTick, SubMsTimer, SubMsTimerCheckpoint};
pub use util::SubMsLcg;

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Per-stage sample buffer + recorder. Optionally annotated with a
/// [`SubMsStageKind`] that sibling adapters (e.g. `subms-otel`) use to pick
/// histogram bucket boundaries.
pub struct SubMsStage {
    name: String,
    samples: Vec<u64>,
    kind: SubMsStageKind,
    // Cheap clones of the harness identity + observer registration so each
    // recorded sample can build an ObservationCtx without borrowing the
    // harness back. None of these allocate on the hot path - the Arc clones
    // happen once at stage construction.
    workload: Arc<str>,
    lang: Arc<str>,
    observer: Option<Arc<dyn SubMsObserver>>,
}

impl SubMsStage {
    fn new(
        name: &str,
        capacity: usize,
        workload: Arc<str>,
        lang: Arc<str>,
        observer: Option<Arc<dyn SubMsObserver>>,
    ) -> Self {
        Self {
            name: name.to_string(),
            samples: Vec::with_capacity(capacity),
            kind: SubMsStageKind::Unspecified,
            workload,
            lang,
            observer,
        }
    }

    /// Annotate this stage's kind so observers can pick fitting histogram
    /// buckets. Default is [`SubMsStageKind::Unspecified`]. Chainable.
    pub fn with_kind(&mut self, kind: SubMsStageKind) -> &mut Self {
        self.kind = kind;
        self
    }

    /// Record an explicit duration in nanoseconds. Also fires the registered
    /// observer (if any).
    pub fn record(&mut self, ns: u64) {
        self.samples.push(ns);
        if let Some(obs) = &self.observer {
            let ctx = ObservationCtx {
                workload: &self.workload,
                lang: &self.lang,
                stage: &self.name,
                stage_kind: self.kind,
            };
            obs.on_record(&ctx, ns);
        }
    }
    /// Time a closure and record its duration.
    pub fn time<F: FnOnce() -> R, R>(&mut self, f: F) -> R {
        let t0 = Instant::now();
        let r = f();
        self.record(t0.elapsed().as_nanos() as u64);
        r
    }

    /// Warm, then record `measured` timed samples of `op`. Runs `op` for
    /// `warmup` untimed iterations first, then times `measured` more. `op`
    /// receives the iteration index on both passes (warmup: `0..warmup`,
    /// measured: `0..measured`); index a shorter input with `i % len`.
    ///
    /// On this AOT-compiled side the warmup mainly primes caches and the
    /// branch predictor. The Java counterpart `warmThenTime` carries the
    /// real weight: it drives HotSpot to C2 (and lets escape analysis elide
    /// short-lived allocations) before any sample is recorded, without which
    /// a JIT-cold low-iteration stage reads orders of magnitude slow. The two
    /// harnesses expose the method symmetrically so a bench reads the same in
    /// either language.
    pub fn warm_then_time<F: FnMut(usize)>(&mut self, warmup: usize, measured: usize, mut op: F) {
        for i in 0..warmup {
            op(i);
        }
        for i in 0..measured {
            let t0 = Instant::now();
            op(i);
            self.record(t0.elapsed().as_nanos() as u64);
        }
    }

    /// Wrap the stage in a coordinated-omission-corrected paced recorder. Each
    /// [`SubMsPacedStage::time`] call blocks until its intended slot, runs the
    /// workload, then records latency from the *intended* start time, which
    /// folds queue delay into the per-op number - the correction
    /// `HdrHistogram` exists for.
    ///
    /// ```ignore
    /// let mut h = SubMsPerfHarness::new("queue", "rust");
    /// let stage = h.stage("offer", 100_000);
    /// let mut paced = stage.with_pacing(10_000.0); // target 10k ops/sec
    /// for _ in 0..100_000 { paced.time(|| do_work()); }
    /// ```
    pub fn with_pacing(&mut self, target_ops_per_second: f64) -> SubMsPacedStage<'_> {
        SubMsPacedStage::new(self, target_ops_per_second)
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn samples(&self) -> &[u64] {
        &self.samples
    }
}

/// Coordinated-omission-corrected stage wrapper. Each [`SubMsPacedStage::time`]
/// call blocks until its intended slot, runs the workload, then records the
/// latency from the *intended* start time to end-of-op (so queue delay is
/// reflected in the per-op latency, not silently dropped).
///
/// Use for benches that simulate constant-throughput arrivals - queues, rate
/// limiters, anything where "if the system stalls, late ops should still count
/// as slow". Java counterpart: `SubMsPerfHarness.SubMsPacedStage`.
pub struct SubMsPacedStage<'a> {
    stage: &'a mut SubMsStage,
    interval_ns: u64,
    started_at: Instant,
    op_index: u64,
}

impl<'a> SubMsPacedStage<'a> {
    fn new(stage: &'a mut SubMsStage, target_ops_per_second: f64) -> Self {
        assert!(
            target_ops_per_second > 0.0,
            "target_ops_per_second must be > 0"
        );
        let interval_ns = ((1_000_000_000.0 / target_ops_per_second) as u64).max(1);
        Self {
            stage,
            interval_ns,
            started_at: Instant::now(),
            op_index: 0,
        }
    }

    /// Time the closure; latency is end-of-op minus *intended* start.
    pub fn time<F: FnOnce() -> R, R>(&mut self, f: F) -> R {
        let intended_start =
            self.started_at + Duration::from_nanos(self.op_index * self.interval_ns);
        let now = Instant::now();
        if now < intended_start {
            thread::sleep(intended_start - now);
        }
        let r = f();
        let end = Instant::now();
        let corrected_latency = end.duration_since(intended_start).as_nanos() as u64;
        self.stage.record(corrected_latency);
        self.op_index += 1;
        r
    }

    pub fn op_index(&self) -> u64 {
        self.op_index
    }
    pub fn interval_ns(&self) -> u64 {
        self.interval_ns
    }
}

/// A workload run. Owns raw samples + metadata only. Analysis and serialisation
/// live in [`crate::bench`] - call [`summarize`] to lift this into a
/// [`SubMsBenchSummary`].
pub struct SubMsPerfHarness {
    // Arc<str> so each Stage holds a cheap clone of the harness identity
    // without per-call string allocation when an observer is registered.
    workload: Arc<str>,
    lang: Arc<str>,
    inputs: BTreeMap<String, String>,
    meta: BTreeMap<String, String>,
    stages: Vec<SubMsStage>,
    observer: Option<Arc<dyn SubMsObserver>>,
    sample_cap: usize,
}

impl SubMsPerfHarness {
    pub fn new(workload: &str, lang: &str) -> Self {
        // The harness is the instrument, so a capture that does not name its
        // version is only reconstructible by luck - the same recipe measured by
        // two harness releases is two experiments. Compile-time, so it cannot
        // drift from the crate it shipped with.
        let mut meta = BTreeMap::new();
        meta.insert(
            "harness_version".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        );
        Self {
            workload: Arc::from(workload),
            lang: Arc::from(lang),
            inputs: BTreeMap::new(),
            meta,
            stages: Vec::new(),
            observer: None,
            sample_cap: 500,
        }
    }

    /// Max points kept in each stage's emitted `samples_ns` timeline. Default
    /// 500; [`crate::benchmark`] sets it from [`crate::SubMsBenchParams::sample_cap`].
    /// Clamped to at least 1.
    pub fn set_sample_cap(&mut self, cap: usize) -> &mut Self {
        self.sample_cap = cap.max(1);
        self
    }

    /// The configured `samples_ns` downsample cap (see [`Self::set_sample_cap`]).
    pub fn sample_cap(&self) -> usize {
        self.sample_cap
    }

    pub fn input(&mut self, key: &str, value: &str) -> &mut Self {
        self.inputs.insert(key.to_string(), value.to_string());
        self
    }

    /// Set a meta field. Renamed from {@code meta} so the {@link Self::meta}
    /// getter can keep its symmetric name with Java's getter.
    pub fn add_meta(&mut self, key: &str, value: &str) -> &mut Self {
        self.meta.insert(key.to_string(), value.to_string());
        self
    }

    /// Create a stage; record samples via [`SubMsStage::time`] or [`SubMsStage::record`].
    pub fn stage(&mut self, name: &str, capacity: usize) -> &mut SubMsStage {
        let stage = SubMsStage::new(
            name,
            capacity,
            Arc::clone(&self.workload),
            Arc::clone(&self.lang),
            self.observer.as_ref().map(Arc::clone),
        );
        self.stages.push(stage);
        self.stages.last_mut().unwrap()
    }

    /// Register an observer to receive every recorded sample and the
    /// post-bench summary. Replaces any existing observer; updates already-
    /// created stages so they fire the new observer too. Returns self for
    /// builder-style chaining.
    pub fn with_observer(mut self, observer: Arc<dyn SubMsObserver>) -> Self {
        self.set_observer(Some(observer));
        self
    }

    /// Mutable setter for late wiring. `None` clears the observer.
    pub fn set_observer(&mut self, observer: Option<Arc<dyn SubMsObserver>>) -> &mut Self {
        for stage in self.stages.iter_mut() {
            stage.observer = observer.as_ref().map(Arc::clone);
        }
        self.observer = observer;
        self
    }

    /// Read the currently-registered observer, if any. Mostly for tests.
    pub fn observer(&self) -> Option<&Arc<dyn SubMsObserver>> {
        self.observer.as_ref()
    }

    /// Borrow a previously-created stage by name.
    pub fn stage_mut(&mut self, name: &str) -> Option<&mut SubMsStage> {
        self.stages.iter_mut().find(|s| s.name == name)
    }

    pub fn stage_by_name(&self, name: &str) -> Option<&SubMsStage> {
        self.stages.iter().find(|s| s.name == name)
    }

    pub fn stages(&self) -> &[SubMsStage] {
        &self.stages
    }

    pub fn workload(&self) -> &str {
        &self.workload
    }
    pub fn lang(&self) -> &str {
        &self.lang
    }
    pub fn inputs(&self) -> &BTreeMap<String, String> {
        &self.inputs
    }
    pub fn meta(&self) -> &BTreeMap<String, String> {
        &self.meta
    }

    /// ISO-8601 seconds-precision timestamp captured at call time. Matches the
    /// on-disk JSON's `timestamp` field.
    pub fn timestamp(&self) -> String {
        iso8601_now()
    }

    /// Back-compat: summarise + emit JSON in the standard subms JSON shape. New
    /// code should call [`summarize`] then [`summary_to_json`] so the
    /// analyser is explicit.
    pub fn write_json<W: Write>(&self, out: &mut W) -> io::Result<()> {
        summary_to_json(&summarize(self), out)
    }

    /// Drop a stage if you never recorded into it.
    pub fn discard_stage(&mut self, name: &str) {
        self.stages.retain(|s| s.name != name);
    }
}

fn iso8601_now() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() as i64;
    let mut year = 1970i64;
    let mut days = secs / 86_400;
    let rem = secs % 86_400;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;
    while days >= year_days(year) {
        days -= year_days(year);
        year += 1;
    }
    let mut month = 1u32;
    for m in 1..=12 {
        let dm = month_days(year, m);
        if days < dm as i64 {
            month = m;
            break;
        }
        days -= dm as i64;
    }
    let day = (days + 1) as u32;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

fn year_days(y: i64) -> i64 {
    if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
        366
    } else {
        365
    }
}
fn month_days(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Parse stdin `key=value` lines into a flat map. Skips blank lines and `#` comments.
pub fn read_stdin_kv() -> BTreeMap<String, String> {
    use std::io::BufRead;
    let mut m = BTreeMap::new();
    let stdin = io::stdin();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            m.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    m
}

#[cfg(test)]
#[path = "subms_tests.rs"]
mod tests;
