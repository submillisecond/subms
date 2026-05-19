use std::thread;
use std::time::Duration;
use subms::SubMsPerfHarness;

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

    // For samples 0..100, p50 ≈ 50, p99 ≈ 99, max = 99.
    assert!(json.contains("\"p50_ns\":50"));
    assert!(json.contains("\"max_ns\":99"));
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
    assert!(samples[1] > samples[0] + 500_000, "delta: first={} second={}", samples[0], samples[1]);
}
