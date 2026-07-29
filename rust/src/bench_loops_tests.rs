//! Unit tests for `bench_loops`, colocated (included via `#[path]` in bench_loops.rs).

use super::*;

#[test]
fn keyed_op_records_count_samples() {
    let mut h = SubMsPerfHarness::new("test", "rust");
    let mut hits = 0usize;
    bench_keyed_op(&mut h, "add", 100, 42, |_key| hits += 1);
    assert_eq!(hits, 100);
    assert_eq!(h.stage_by_name("add").unwrap().samples().len(), 100);
}

#[test]
fn keyed_op_is_deterministic_under_same_seed() {
    let mut h1 = SubMsPerfHarness::new("a", "rust");
    let mut h2 = SubMsPerfHarness::new("b", "rust");
    let mut seen1: Vec<String> = Vec::new();
    let mut seen2: Vec<String> = Vec::new();
    bench_keyed_op(&mut h1, "x", 20, 7, |k| seen1.push(k.to_string()));
    bench_keyed_op(&mut h2, "x", 20, 7, |k| seen2.push(k.to_string()));
    assert_eq!(seen1, seen2);
}

#[test]
fn indexed_op_passes_sequential_indices() {
    let mut h = SubMsPerfHarness::new("test", "rust");
    let mut last = -1i64;
    let mut count = 0usize;
    bench_indexed_op(&mut h, "scan", 50, |i| {
        assert!((i as i64) > last);
        last = i as i64;
        count += 1;
    });
    assert_eq!(count, 50);
}

#[test]
fn templated_op_substitutes_index() {
    let mut h = SubMsPerfHarness::new("test", "rust");
    let mut keys = Vec::new();
    bench_templated_op(&mut h, "miss", 5, "absent-{}", |k| {
        keys.push(k.to_string());
    });
    assert_eq!(
        keys,
        vec!["absent-0", "absent-1", "absent-2", "absent-3", "absent-4"]
    );
}
