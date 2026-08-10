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
fn lcg_adjacent_seeds_do_not_alias() {
    // Recipes warm on `seed` and measure on `seed + 1`; if those two streams
    // coincide the measured keys are the warm-up keys replayed.
    for base in [0u64, 1, 7, 42, u64::MAX - 1] {
        let mut a = SubMsLcg::new(base);
        let mut b = SubMsLcg::new(base.wrapping_add(1));
        let sa: Vec<u32> = (0..1000).map(|_| a.next_u32()).collect();
        let sb: Vec<u32> = (0..1000).map(|_| b.next_u32()).collect();
        assert_ne!(sa, sb, "seeds {base} and {}+1 produced one stream", base);
        let overlap = sa.iter().filter(|k| sb.contains(k)).count();
        assert!(
            overlap < 10,
            "seed {base} overlaps its successor {overlap} times"
        );
    }
}

#[test]
fn lcg_seed_zero_has_full_entropy() {
    let mut rng = SubMsLcg::new(0);
    let draws: Vec<u32> = (0..1000).map(|_| rng.next_u32()).collect();
    assert!(draws.iter().any(|&v| v != draws[0]));
    assert!(draws.iter().collect::<std::collections::HashSet<_>>().len() > 990);
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
