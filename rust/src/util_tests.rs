//! Unit tests for `util`, colocated (included via `#[path]` in util.rs).

use super::*;

#[test]
fn lcg_is_deterministic() {
    let mut a = SubMsLcg::new(42);
    let mut b = SubMsLcg::new(42);
    for _ in 0..1000 {
        assert_eq!(a.next_u32(), b.next_u32());
    }
}

#[test]
fn lcg_bounded_stays_in_range() {
    let mut rng = SubMsLcg::new(7);
    for _ in 0..10_000 {
        assert!(rng.bounded(100) < 100);
    }
}

#[test]
fn lcg_bounded_zero_is_safe() {
    let mut rng = SubMsLcg::new(7);
    assert_eq!(rng.bounded(0), 0);
}
