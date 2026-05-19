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
pub mod params;
pub mod recipe;
pub mod summary;
pub mod timer;
pub mod util;

pub use bench::{
    assert_p99_under, diff_summary, diff_summary_with, diff_to_json, format_ns, percentile,
    print_diff, print_summary, print_sweep, run_bench, run_sweep, summarize, summarize_lean,
    summarize_sweep, summary_to_json, sweep_to_json, SubMsBenchAssertion,
    DEFAULT_REGRESSION_THRESHOLD_PCT,
};
pub use params::{parse_bool, parse_string, parse_u64, parse_usize};
pub use recipe::{benchmark, SubMsBenchParams, SubMsRecipe};
pub use summary::{
    SubMsBenchDiff, SubMsBenchSummary, SubMsBenchSweep, SubMsMetricDiff, SubMsStageDiff,
    SubMsStageSummary,
};
// SubMsPacedStage is defined inline in this file - re-export it from the root.
pub use timer::{SubMsTimer, SubMsTimerCheckpoint};
pub use util::SubMsLcg;

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Per-stage sample buffer + recorder.
pub struct SubMsStage {
    name: String,
    samples: Vec<u64>,
}

impl SubMsStage {
    fn new(name: &str, capacity: usize) -> Self {
        Self { name: name.to_string(), samples: Vec::with_capacity(capacity) }
    }
    /// Record an explicit duration in nanoseconds.
    pub fn record(&mut self, ns: u64) {
        self.samples.push(ns);
    }
    /// Time a closure and record its duration.
    pub fn time<F: FnOnce() -> R, R>(&mut self, f: F) -> R {
        let t0 = Instant::now();
        let r = f();
        self.samples.push(t0.elapsed().as_nanos() as u64);
        r
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

    pub fn name(&self) -> &str { &self.name }
    pub fn samples(&self) -> &[u64] { &self.samples }
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
        assert!(target_ops_per_second > 0.0, "target_ops_per_second must be > 0");
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
        let intended_start = self.started_at + Duration::from_nanos(self.op_index * self.interval_ns);
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

    pub fn op_index(&self) -> u64 { self.op_index }
    pub fn interval_ns(&self) -> u64 { self.interval_ns }
}

/// A workload run. Owns raw samples + metadata only. Analysis and serialisation
/// live in [`crate::bench`] - call [`summarize`] to lift this into a
/// [`SubMsBenchSummary`].
pub struct SubMsPerfHarness {
    workload: String,
    lang: String,
    inputs: BTreeMap<String, String>,
    meta: BTreeMap<String, String>,
    stages: Vec<SubMsStage>,
}

impl SubMsPerfHarness {
    pub fn new(workload: &str, lang: &str) -> Self {
        Self {
            workload: workload.to_string(),
            lang: lang.to_string(),
            inputs: BTreeMap::new(),
            meta: BTreeMap::new(),
            stages: Vec::new(),
        }
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
        self.stages.push(SubMsStage::new(name, capacity));
        self.stages.last_mut().unwrap()
    }

    /// Borrow a previously-created stage by name.
    pub fn stage_mut(&mut self, name: &str) -> Option<&mut SubMsStage> {
        self.stages.iter_mut().find(|s| s.name == name)
    }

    pub fn stage_by_name(&self, name: &str) -> Option<&SubMsStage> {
        self.stages.iter().find(|s| s.name == name)
    }

    pub fn stages(&self) -> &[SubMsStage] { &self.stages }

    pub fn workload(&self) -> &str { &self.workload }
    pub fn lang(&self) -> &str { &self.lang }
    pub fn inputs(&self) -> &BTreeMap<String, String> { &self.inputs }
    pub fn meta(&self) -> &BTreeMap<String, String> { &self.meta }

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
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
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
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hour, minute, second)
}

fn year_days(y: i64) -> i64 {
    if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) { 366 } else { 365 }
}
fn month_days(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) { 29 } else { 28 },
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
