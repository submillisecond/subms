//! Unit tests for `bench`, colocated (included via `#[path]` in bench.rs).

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
