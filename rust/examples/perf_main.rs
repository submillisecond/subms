//! Self-bench. Measures the harness's own hot paths and emits a
//! `SubMsBenchSummary` on stdout. Driven by `subms-action-bench` in this
//! repo's perf workflow - if a future PR slows down the recording overhead
//! by more than the per-stage threshold, the gate catches it on the PR.
//!
//! Run locally:
//!   cargo run --release --example perf_main > perf.json
//!
//! Stages:
//!   time_closure   - end-to-end cost of `stage.time(|| {})`. The floor for
//!                    any user who instruments their hot path with `time()`.
//!   record_ns      - just the Vec push (`stage.record(ns)`). Strict lower
//!                    bound on per-sample bookkeeping.
//!   summarize      - sort + percentile extraction over 50k samples. Cost
//!                    of producing one `SubMsBenchSummary`.
//!   summary_to_json - serialisation of one summary into the standard JSON
//!                    shape. Cost paid by every CI run that uploads results.
//!   diff_summary   - regression-diff math between two summaries. Cost paid
//!                    by every PR-time gate.

use std::hint::black_box;
use std::io::{Cursor, Write};

use subms::{SubMsBenchSummary, SubMsPerfHarness, SubMsTimer, summarize, summary_to_json};

fn main() {
    let mut h = SubMsPerfHarness::new("subms-self-bench", "rust");
    h.input("samples_per_stage", "50000");
    h.add_meta("rust_version", env!("CARGO_PKG_RUST_VERSION"));
    h.add_meta("crate_version", env!("CARGO_PKG_VERSION"));

    // 1. time_closure: cost of `stage.time(|| {})` with an empty closure.
    //    This is the per-iteration overhead any user pays when they wrap
    //    their hot path with `stage.time(...)`.
    {
        let s = h.stage("time_closure", 50_000);
        for _ in 0..50_000 {
            s.time(|| black_box(()));
        }
    }

    // 2. record_ns: cost of `stage.record(ns)` alone. No timer, just the
    //    Vec push. Strict lower bound on per-sample bookkeeping.
    {
        let s = h.stage("record_ns", 50_000);
        for i in 0..50_000u64 {
            s.record(black_box(100 + (i & 63)));
        }
    }

    // 3. summarize: sort + percentile extraction. Cost of producing one
    //    SubMsBenchSummary. We use SubMsTimer in a parallel harness so the
    //    measurement of the measurement isn't recursive.
    {
        let summary_target = build_sample_harness(50_000);
        let s = h.stage("summarize", 100);
        for _ in 0..100 {
            s.time(|| {
                let summary = summarize(black_box(&summary_target));
                black_box(summary);
            });
        }
    }

    // 4. summary_to_json: serialise to the canonical JSON shape. Cost paid
    //    by every CI run that uploads results.
    {
        let summary = summarize(&build_sample_harness(50_000));
        let s = h.stage("summary_to_json", 100);
        for _ in 0..100 {
            s.time(|| {
                let mut buf = Cursor::new(Vec::with_capacity(64 * 1024));
                summary_to_json(black_box(&summary), &mut buf).unwrap();
                let _ = buf.flush();
                black_box(buf);
            });
        }
    }

    // 5. diff_summary: regression-diff math between two SubMsBenchSummary
    //    objects. Cost paid by every PR-time gate.
    {
        let base: SubMsBenchSummary = summarize(&build_sample_harness(50_000));
        let cand: SubMsBenchSummary = summarize(&build_sample_harness(50_000));
        let s = h.stage("diff_summary", 1_000);
        for _ in 0..1_000 {
            s.time(|| {
                let d = subms::diff_summary(black_box(&base), black_box(&cand));
                black_box(d);
            });
        }
    }

    // Cross-check: SubMsTimer itself rolls a quick checkpoint walk so the
    // timer's marking overhead is also visible from this perf JSON's meta.
    let mut t = SubMsTimer::new("self-bench-wall");
    t.mark("stages-complete");
    t.stop("emitting-json");
    h.add_meta("self_bench_wall_ns", &t.elapsed_ns().to_string());

    let summary = summarize(&h);
    summary_to_json(&summary, &mut std::io::stdout()).unwrap();
}

/// Build a SubMsPerfHarness pre-populated with a synthetic stage's worth of
/// samples - used as fodder for the summarize / json / diff stages above.
fn build_sample_harness(n: usize) -> SubMsPerfHarness {
    let mut h = SubMsPerfHarness::new("fixture", "rust");
    let s = h.stage("fixture-stage", n);
    for i in 0..n as u64 {
        // Synthetic log-ish distribution: most samples small, a tail
        // exercises percentile sort + lookup.
        let v = if i % 1000 == 0 {
            10_000 + (i & 1023)
        } else {
            100 + (i & 63)
        };
        s.record(v);
    }
    h
}
