//! Unit tests for `timer`, colocated (included via `#[path]` in timer.rs).

use super::*;
use std::thread;
use std::time::Duration;

#[test]
fn autostart_and_mark_captures_increasing_since_start() {
    let mut t = SubMsTimer::new("x");
    thread::sleep(Duration::from_millis(1));
    let a = t.mark("a");
    thread::sleep(Duration::from_millis(1));
    let b = t.mark("b");
    assert!(a > 0);
    assert!(b > a);
    assert_eq!(t.checkpoints().len(), 2);
    assert_eq!(t.checkpoints()[0].label, "a");
    assert_eq!(t.checkpoints()[1].label, "b");
    assert!(!t.checkpoints()[0].is_stop);
}

#[test]
fn stop_marks_is_stop_and_freezes_elapsed() {
    let mut t = SubMsTimer::new("x");
    thread::sleep(Duration::from_millis(1));
    t.stop("done");
    assert!(t.is_stopped());
    let e1 = t.elapsed_ns();
    thread::sleep(Duration::from_millis(2));
    let e2 = t.elapsed_ns();
    assert_eq!(e1, e2, "elapsed should freeze after stop");
    assert!(t.checkpoints().last().unwrap().is_stop);
}

#[test]
fn reset_clears_checkpoints() {
    let mut t = SubMsTimer::new("x");
    t.mark("a");
    t.mark("b");
    t.reset();
    assert!(t.checkpoints().is_empty());
    assert!(!t.is_stopped());
}

#[test]
fn lap_is_alias_of_mark() {
    let mut t = SubMsTimer::new("x");
    t.lap("a");
    assert_eq!(t.checkpoints().len(), 1);
    assert_eq!(t.checkpoints()[0].label, "a");
}

#[test]
fn print_emits_header_and_checkpoints() {
    let mut t = SubMsTimer::new("parse");
    t.mark("a");
    t.stop("done");
    let mut buf = Vec::new();
    t.print(&mut buf).unwrap();
    let out = String::from_utf8(buf).unwrap();
    assert!(out.contains("timer \"parse\""));
    assert!(out.contains("a"));
    assert!(out.contains("done *"));
}

// ---------------- static clock API ----------------

#[test]
fn nanos_now_returns_positive_increasing() {
    let a = SubMsTimer::nanos_now();
    thread::sleep(Duration::from_millis(1));
    let b = SubMsTimer::nanos_now();
    assert!(b > a, "monotonic: {} -> {}", a, b);
}

#[test]
fn tick_and_elapsed_ns_capture_positive_interval() {
    let t = SubMsTimer::tick();
    thread::sleep(Duration::from_millis(2));
    let ns = t.elapsed_ns();
    assert!(ns >= 1_000_000, "should be >= 1ms after sleep: {}", ns);
    assert!(
        ns < 100_000_000,
        "shouldn't be > 100ms on a healthy box: {}",
        ns
    );
}

#[test]
fn tick_is_reusable_for_multiple_reads() {
    let t = SubMsTimer::tick();
    thread::sleep(Duration::from_millis(1));
    let a = t.elapsed_ns();
    thread::sleep(Duration::from_millis(1));
    let b = t.elapsed_ns();
    assert!(b >= a, "second read >= first: {} -> {}", a, b);
}

#[test]
fn measure_ns_returns_elapsed_and_runs_closure() {
    let mut counter = 0;
    let ((), elapsed) = SubMsTimer::measure_ns(|| {
        counter += 1;
        thread::sleep(Duration::from_millis(1));
    });
    assert_eq!(counter, 1, "closure should run exactly once");
    assert!(
        elapsed >= 500_000,
        "elapsed should be at least 0.5ms: {}",
        elapsed
    );
}

#[test]
fn measure_ns_propagates_closure_return_value() {
    let (val, _ns) = SubMsTimer::measure_ns(|| 42);
    assert_eq!(val, 42);
}

#[test]
fn measure_ns_returns_zero_or_positive_for_noop() {
    let (_, elapsed) = SubMsTimer::measure_ns(|| {});
    assert!(
        elapsed < 1_000_000,
        "no-op shouldn't take >= 1ms: {}",
        elapsed
    );
}
