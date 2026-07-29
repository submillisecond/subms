//! Unit tests for `growth`, colocated (included via `#[path]` in growth.rs).

use super::*;

// A recipe whose on-disk footprint per round is scripted, so the verdict
// logic can be exercised deterministically. live_bytes is fixed (a flat
// working set); on_disk follows `disk[round-1]`.
struct ScriptedRecipe {
    disk: Vec<u64>,
    live: u64,
    class: SubMsGrowthClass,
    bound: f64,
    round: usize,
}
impl SubMsGrowthRecipe for ScriptedRecipe {
    fn name(&self) -> &str {
        "scripted"
    }
    fn rounds(&self) -> usize {
        self.disk.len()
    }
    fn ops_per_round(&self) -> usize {
        4
    }
    fn op(&mut self, round: usize, _i: usize) {
        self.round = round;
    }
    fn disk_bytes(&mut self) -> u64 {
        self.disk[self.round - 1]
    }
    fn live_bytes(&mut self) -> u64 {
        self.live
    }
    fn structures(&mut self) -> Vec<(String, u64)> {
        vec![("sstables".to_string(), self.round as u64)]
    }
    fn expected(&self) -> (SubMsGrowthClass, f64) {
        (self.class, self.bound)
    }
}

#[test]
fn amplification_bounded_holds_when_disk_tracks_live() {
    // on-disk stays within ~1.2x of a flat 1000-byte live set -> holds.
    let mut r = ScriptedRecipe {
        disk: vec![1000, 1100, 1050, 1200],
        live: 1000,
        class: SubMsGrowthClass::AmplificationBounded,
        bound: 3.0,
        round: 0,
    };
    let report = grow(&mut r, "rust");
    assert!(report.verdict.holds, "{}", report.verdict.summary);
    assert!((report.verdict.observed - 1.2).abs() < 1e-9);
    assert!(assert_growth_holds(&report).is_ok());
}

#[test]
fn amplification_bounded_breaches_when_disk_grows_but_live_flat() {
    // the leak shape: live flat at 1000, on-disk climbs to 21x -> breach.
    let mut r = ScriptedRecipe {
        disk: vec![1000, 5000, 12000, 21000],
        live: 1000,
        class: SubMsGrowthClass::AmplificationBounded,
        bound: 3.0,
        round: 0,
    };
    let report = grow(&mut r, "rust");
    assert!(!report.verdict.holds);
    assert!((report.verdict.observed - 21.0).abs() < 1e-9);
    assert!(assert_growth_holds(&report).is_err());
}

#[test]
fn plateau_holds_when_flat_and_breaches_when_climbing() {
    // Flat file (pre-allocated, then steady) -> holds even though absolute
    // size dwarfs the tiny live set.
    let mut flat = ScriptedRecipe {
        disk: vec![66_000, 66_000, 66_000, 66_000],
        live: 20,
        class: SubMsGrowthClass::PlateauBounded,
        bound: 1.5,
        round: 0,
    };
    let held = grow(&mut flat, "rust");
    assert!(held.verdict.holds, "{}", held.verdict.summary);

    // File that keeps climbing through the second half -> breach (a leak).
    // Baseline is the mid-point (round 3 here); last / mid = 16000 / 4000 = 4x.
    let mut climbing = ScriptedRecipe {
        disk: vec![1000, 2000, 4000, 16000],
        live: 20,
        class: SubMsGrowthClass::PlateauBounded,
        bound: 1.5,
        round: 0,
    };
    let leak = grow(&mut climbing, "rust");
    assert!(!leak.verdict.holds);
    assert!((leak.verdict.observed - 4.0).abs() < 1e-9);
}

#[test]
fn bounded_gates_on_peak_bytes() {
    let mut r = ScriptedRecipe {
        disk: vec![100, 128, 128, 128],
        live: 128,
        class: SubMsGrowthClass::Bounded,
        bound: 128.0,
        round: 0,
    };
    let report = grow(&mut r, "rust");
    assert!(report.verdict.holds);
}

#[test]
fn json_has_verdict_and_rounds() {
    let mut r = ScriptedRecipe {
        disk: vec![1000, 1100],
        live: 1000,
        class: SubMsGrowthClass::AmplificationBounded,
        bound: 3.0,
        round: 0,
    };
    let report = grow(&mut r, "rust");
    let mut buf: Vec<u8> = Vec::new();
    growth_to_json(&report, &mut buf).unwrap();
    let s = String::from_utf8(buf).unwrap();
    assert!(s.contains("\"kind\":\"growth\""));
    assert!(s.contains("\"class\":\"amplification_bounded\""));
    assert!(s.contains("\"holds\":true"));
    assert!(s.contains("\"sstables\":2"));
    assert!(s.contains("\"amplification\":"));
}
