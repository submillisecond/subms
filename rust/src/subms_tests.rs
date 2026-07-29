use crate::SubMsPerfHarness;
use std::thread;
use std::time::Duration;

#[test]
fn round_trip_smoke() {
    let mut h = SubMsPerfHarness::new("toy", "rust");
    h.input("entries", "1000");
    h.input("bloom_mode", "on");
    h.add_meta("sstables", "1");

    let s = h.stage("work", 1000);
    for i in 0..1000u64 {
        s.record(i * 10);
    }

    let mut buf = Vec::new();
    h.write_json(&mut buf).unwrap();
    let json = String::from_utf8(buf).unwrap();

    assert!(json.starts_with('{'));
    assert!(json.contains("\"workload\":\"toy\""));
    assert!(json.contains("\"lang\":\"rust\""));
    assert!(json.contains("\"inputs\""));
    assert!(json.contains("\"entries\":\"1000\""));
    assert!(json.contains("\"work\":{"));
    assert!(json.contains("\"count\":1000"));
    assert!(json.contains("\"samples_ns\":["));
}

#[test]
fn percentiles_make_sense() {
    let mut h = SubMsPerfHarness::new("perc", "rust");
    let s = h.stage("op", 100);
    for i in 0..100u64 {
        s.record(i);
    }

    let mut buf = Vec::new();
    h.write_json(&mut buf).unwrap();
    let json = String::from_utf8(buf).unwrap();

    // For samples 0..100, p50 ~ 50, p99 ~ 99, max = 99.
    assert!(json.contains("\"p50_ns\":50"));
    assert!(json.contains("\"max_ns\":99"));
}

#[test]
fn warm_then_time_records_measured_only() {
    let mut h = SubMsPerfHarness::new("warm", "rust");
    let s = h.stage("op", 8);
    let mut calls = 0usize;
    let mut last_idx = 0usize;
    s.warm_then_time(100, 8, |i| {
        calls += 1;
        last_idx = i;
    });
    // op ran warmup + measured times, but only the measured pass is sampled.
    assert_eq!(calls, 108, "op ran for warmup + measured iterations");
    assert_eq!(
        s.samples().len(),
        8,
        "only the measured pass produced samples"
    );
    assert_eq!(last_idx, 7, "measured pass walks 0..measured-1");
}

// ---------- SubMsObserver integration ----------------------------------------

use crate::{ObservationCtx, SubMsObserver, SubMsStageKind, summarize};
use std::sync::{Arc, Mutex};

/// One captured record. Named struct rather than a tuple so clippy stays
/// quiet about type complexity and the test assertions read cleanly.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Captured {
    stage: String,
    ns: u64,
    kind: SubMsStageKind,
    workload: String,
    lang: String,
}

/// Test observer that records every call. Wrapped in `Arc<Mutex<...>>` so
/// the observer (passed in as `Arc<dyn SubMsObserver>`) shares state with
/// the asserting test code.
#[derive(Default)]
struct RecordingObserver {
    records: Mutex<Vec<Captured>>,
    summaries: Mutex<u32>,
}

impl SubMsObserver for RecordingObserver {
    fn on_record(&self, ctx: &ObservationCtx, ns: u64) {
        self.records.lock().unwrap().push(Captured {
            stage: ctx.stage.to_string(),
            ns,
            kind: ctx.stage_kind,
            workload: ctx.workload.to_string(),
            lang: ctx.lang.to_string(),
        });
    }
    fn on_summarize(&self, _summary: &crate::SubMsBenchSummary) {
        *self.summaries.lock().unwrap() += 1;
    }
}

#[test]
fn observer_default_noop_does_not_change_behaviour() {
    // No observer set - harness behaves identically to today: stage.record
    // pushes samples, summary builds, no panics.
    let mut h = SubMsPerfHarness::new("noop", "rust");
    let s = h.stage("op", 4);
    s.record(100);
    s.record(200);
    assert_eq!(s.samples().len(), 2);
    let summary = summarize(&h);
    assert_eq!(summary.stages.len(), 1);
}

#[test]
fn observer_fires_on_record_and_time() {
    let obs = Arc::new(RecordingObserver::default());
    let mut h = SubMsPerfHarness::new("rec", "rust").with_observer(obs.clone());
    let s = h.stage("op", 4);
    s.record(42);
    s.time(|| {});
    let captured = obs.records.lock().unwrap().clone();
    assert_eq!(captured.len(), 2, "record + time each fire once");
    assert_eq!(captured[0].stage, "op");
    assert_eq!(captured[0].ns, 42);
    assert_eq!(captured[0].workload, "rec");
    assert_eq!(captured[0].lang, "rust");
    // The time() sample is whatever Instant::now measured - just check it
    // came through with the right stage name.
    assert_eq!(captured[1].stage, "op");
}

#[test]
fn observer_fires_on_warm_then_time_only_for_measured_pass() {
    let obs = Arc::new(RecordingObserver::default());
    let mut h = SubMsPerfHarness::new("warm", "rust").with_observer(obs.clone());
    let s = h.stage("op", 8);
    s.warm_then_time(50, 8, |_| {});
    let n = obs.records.lock().unwrap().len();
    // Warmup pass is untimed and must not fire the observer; only the
    // measured pass produces records.
    assert_eq!(n, 8, "observer fires only on the measured pass");
}

#[test]
fn observer_records_carry_declared_stage_kind() {
    let obs = Arc::new(RecordingObserver::default());
    let mut h = SubMsPerfHarness::new("kind", "rust").with_observer(obs.clone());
    h.stage("put", 4)
        .with_kind(SubMsStageKind::HotPath)
        .record(10);
    h.stage("compact", 4)
        .with_kind(SubMsStageKind::BatchOp)
        .record(20);
    let captured = obs.records.lock().unwrap().clone();
    assert_eq!(captured[0].kind, SubMsStageKind::HotPath);
    assert_eq!(captured[1].kind, SubMsStageKind::BatchOp);
}

#[test]
fn observer_fires_on_summarize_exactly_once() {
    let obs = Arc::new(RecordingObserver::default());
    let mut h = SubMsPerfHarness::new("sum", "rust").with_observer(obs.clone());
    let s = h.stage("op", 4);
    s.record(1);
    s.record(2);
    let _summary = summarize(&h);
    assert_eq!(*obs.summaries.lock().unwrap(), 1);
}

#[test]
fn set_observer_updates_already_created_stages() {
    // A stage created BEFORE the observer is installed should still fire
    // the observer once set_observer wires it up.
    let mut h = SubMsPerfHarness::new("late", "rust");
    let s = h.stage("op", 4);
    s.record(1); // before observer - silent
    let obs = Arc::new(RecordingObserver::default());
    h.set_observer(Some(obs.clone()));
    h.stage_mut("op").unwrap().record(2);
    let captured = obs.records.lock().unwrap().clone();
    assert_eq!(captured.len(), 1, "only the post-install record fires");
    assert_eq!(captured[0].ns, 2);
}

#[test]
fn paced_stage_folds_queue_delay_into_latency() {
    let mut h = SubMsPerfHarness::new("paced", "rust");
    let stage = h.stage("op", 8);
    let mut paced = stage.with_pacing(1_000.0); // 1ms interval

    // First op: should fall close to its slot.
    paced.time(|| {});
    // Stall 2ms - second op fires late; its corrected latency includes the gap.
    thread::sleep(Duration::from_millis(2));
    paced.time(|| {});

    let samples = h.stage_by_name("op").unwrap().samples();
    assert_eq!(samples.len(), 2);
    // First op below the 1ms interval.
    assert!(samples[0] < 1_000_000, "first: {}", samples[0]);
    // Second op should reflect the ~2ms slot delay (CO correction).
    assert!(samples[1] > 1_000_000, "second: {}", samples[1]);
    assert!(
        samples[1] > samples[0] + 500_000,
        "delta: first={} second={}",
        samples[0],
        samples[1]
    );
}
