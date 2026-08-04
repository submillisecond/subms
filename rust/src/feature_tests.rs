//! Unit tests for the per-feature classification + manifest. Colocated with the
//! module and included via `#[path]` (see `feature.rs`), so they live alongside
//! the code and can reach internals if needed. Mirrors `SubMsFeatureManifestTest`.

use std::collections::BTreeMap;

use super::*;

#[test]
fn classifies_flat_above_base_as_hot_path() {
    let sweep = [(1_024usize, 300u64), (65_536usize, 320u64)];
    let (cat, _) = classify_feature(&sweep, Some(50), None);
    assert_eq!(cat, SubMsFeatureCategory::HotPath);
}

#[test]
fn classifies_linear_growth_as_structural() {
    let sweep = [(1_024usize, 1_000u64), (65_536usize, 64_000u64)];
    let (cat, _) = classify_feature(&sweep, None, None);
    assert_eq!(cat, SubMsFeatureCategory::Structural);
}

#[test]
fn log_n_growth_stays_hot_path() {
    // p99 grows ~6x (log2 of 64x) - well under the 0.5*63 structural bar.
    let sweep = [(1_024usize, 100u64), (65_536usize, 600u64)];
    let (cat, _) = classify_feature(&sweep, Some(50), None);
    assert_eq!(cat, SubMsFeatureCategory::HotPath);
}

#[test]
fn no_workload_is_auxiliary() {
    let (cat, _) = classify_feature(&[], Some(300), None);
    assert_eq!(cat, SubMsFeatureCategory::Auxiliary);
}

#[test]
fn no_delta_from_base_is_auxiliary() {
    // Flat and within 10% of base -> measured non-effect.
    let sweep = [(1_024usize, 305u64), (65_536usize, 308u64)];
    let (cat, _) = classify_feature(&sweep, Some(300), None);
    assert_eq!(cat, SubMsFeatureCategory::Auxiliary);
}

#[test]
fn override_wins() {
    let sweep = [(1_024usize, 1_000u64), (65_536usize, 64_000u64)];
    let (cat, why) = classify_feature(&sweep, None, Some(SubMsFeatureCategory::HotPath));
    assert_eq!(cat, SubMsFeatureCategory::HotPath);
    assert!(why.contains("override"));
}

#[test]
fn merge_preserves_unknown_fields() {
    let existing = r#"{
      "lang": "rust",
      "customTop": 42,
      "features": {
        "counting": { "perf": "auxiliary", "note": "keep me", "p99ByStage": { "add": 1 } },
        "other": { "perf": "hot-path" }
      }
    }"#;
    let mut m = SubMsFeatureManifest::load_str("rust", existing);
    let mut p99 = BTreeMap::new();
    p99.insert("add".to_string(), 320u64);
    p99.insert("remove".to_string(), 355u64);
    m.set_feature("counting", SubMsFeatureCategory::HotPath, &p99, "measured");
    let out = m.to_json();
    assert!(out.contains("\"perf\": \"hot-path\""));
    assert!(out.contains("\"remove\": 355"));
    assert!(out.contains("\"note\": \"keep me\""));
    assert!(out.contains("\"customTop\": 42"));
    assert!(out.contains("\"other\""));
    assert_eq!(
        m.category_of("counting"),
        Some(SubMsFeatureCategory::HotPath)
    );
}

#[test]
fn structural_feature_drops_p99() {
    let mut m = SubMsFeatureManifest::new("rust");
    let mut p99 = BTreeMap::new();
    p99.insert("serialize".to_string(), 5_000u64);
    m.set_feature("serialize", SubMsFeatureCategory::HotPath, &p99, "x");
    m.set_feature(
        "serialize",
        SubMsFeatureCategory::Structural,
        &BTreeMap::new(),
        "O(n)+",
    );
    let out = m.to_json();
    assert!(out.contains("\"perf\": \"structural\""));
    assert!(!out.contains("p99ByStage"));
}

#[test]
fn round_trips_number_precision() {
    let existing = r#"{"features":{"f":{"p99ByStage":{"add":9007199254740993}}}}"#;
    let m = SubMsFeatureManifest::load_str("rust", existing);
    assert!(m.to_json().contains("9007199254740993"));
}

#[test]
fn merge_preserves_deep_third_party_structures() {
    let existing = r#"{
      "lang": "rust",
      "schemaVersion": 3,
      "vendor": { "name": "acme", "tags": ["a", "b"], "nested": { "deep": [1, 2, 3] } },
      "features": {
        "counting": {
          "perf": "auxiliary",
          "owner": "team-x",
          "customArr": [{ "k": "v" }, 7, true, null],
          "p99ByStage": { "add": 1 }
        },
        "scalable": { "perf": "hot-path", "vendorNote": "do not drop" }
      }
    }"#;
    let mut m = SubMsFeatureManifest::load_str("rust", existing);
    let mut p99 = BTreeMap::new();
    p99.insert("add".to_string(), 320u64);
    m.set_feature("counting", SubMsFeatureCategory::HotPath, &p99, "measured");
    let s = m.to_json();

    assert!(matches!(parse_json(&s), Ok(Json::Obj(_))));
    assert!(s.contains("\"schemaVersion\": 3"));
    assert!(s.contains("\"name\": \"acme\""));
    assert!(s.contains("\"deep\": ["));
    assert!(s.contains("\"customArr\""));
    assert!(s.contains("\"owner\": \"team-x\""));
    assert!(s.contains("null"));
    assert!(s.contains("true"));
    assert!(s.contains("\"vendorNote\": \"do not drop\""));
    assert_eq!(
        m.category_of("counting"),
        Some(SubMsFeatureCategory::HotPath)
    );
    assert_eq!(
        m.category_of("scalable"),
        Some(SubMsFeatureCategory::HotPath)
    );
    assert!(s.contains("\"add\": 320"));
}

#[test]
fn repeated_merges_are_lossless() {
    let mut m = SubMsFeatureManifest::load_str("rust", r#"{"vendor":{"x":1},"features":{}}"#);
    for (name, cat) in [
        ("a", SubMsFeatureCategory::HotPath),
        ("b", SubMsFeatureCategory::Structural),
        ("c", SubMsFeatureCategory::Auxiliary),
    ] {
        m.set_feature(name, cat, &BTreeMap::new(), "r");
    }
    let mut p99 = BTreeMap::new();
    p99.insert("op".to_string(), 42u64);
    m.set_feature("a", SubMsFeatureCategory::HotPath, &p99, "r2");
    let s = m.to_json();
    assert!(s.contains("\"vendor\""));
    assert!(s.contains("\"x\": 1"));
    assert_eq!(m.category_of("a"), Some(SubMsFeatureCategory::HotPath));
    assert_eq!(m.category_of("b"), Some(SubMsFeatureCategory::Structural));
    assert_eq!(m.category_of("c"), Some(SubMsFeatureCategory::Auxiliary));
    assert!(s.contains("\"op\": 42"));
}

#[test]
fn sample_features_classify_correctly() {
    // Realistic archetypes: the harness decides each from its measured shape,
    // proving the taxonomy is an output, not a hand-authored label.
    let base = Some(300u64);
    type SampleCase = (&'static str, Vec<(usize, u64)>, SubMsFeatureCategory);
    let cases: [SampleCase; 5] = [
        (
            "counting",
            vec![(1_024, 350), (16_384, 360), (262_144, 372)],
            SubMsFeatureCategory::HotPath,
        ),
        (
            "serialize",
            vec![(1_024, 12_000), (16_384, 190_000), (262_144, 3_050_000)],
            SubMsFeatureCategory::Structural,
        ),
        ("serde", vec![], SubMsFeatureCategory::Auxiliary),
        (
            "metrics",
            vec![(1_024, 308), (262_144, 312)],
            SubMsFeatureCategory::Auxiliary,
        ),
        (
            "persistent",
            vec![(1_024, 400), (262_144, 900)],
            SubMsFeatureCategory::Structural,
        ),
    ];
    let mut m = SubMsFeatureManifest::new("rust");
    for (name, sweep, expect) in &cases {
        let ovr = if *name == "persistent" {
            Some(SubMsFeatureCategory::Structural)
        } else {
            None
        };
        let (cat, reason) = classify_feature(sweep, base, ovr);
        assert_eq!(cat, *expect, "feature {name}: {reason}");
        let mut p99 = BTreeMap::new();
        if cat == SubMsFeatureCategory::HotPath {
            if let Some((_, v)) = sweep.last() {
                p99.insert("op".to_string(), *v);
            }
        }
        m.set_feature(name, cat, &p99, &reason);
    }
    assert_eq!(
        m.category_of("counting"),
        Some(SubMsFeatureCategory::HotPath)
    );
    assert_eq!(
        m.category_of("serialize"),
        Some(SubMsFeatureCategory::Structural)
    );
    assert_eq!(
        m.category_of("serde"),
        Some(SubMsFeatureCategory::Auxiliary)
    );
    assert_eq!(
        m.category_of("metrics"),
        Some(SubMsFeatureCategory::Auxiliary)
    );
    assert_eq!(
        m.category_of("persistent"),
        Some(SubMsFeatureCategory::Structural)
    );
}

#[test]
fn malformed_input_never_crashes_and_stays_writable() {
    for bad in [
        "",
        "   ",
        "not json",
        "[1,2,3]",
        "{\"features\":",
        "42",
        "null",
        "\"a string\"",
    ] {
        let mut m = SubMsFeatureManifest::load_str("rust", bad);
        assert!(
            matches!(parse_json(&m.to_json()), Ok(Json::Obj(_))),
            "bad input {bad:?} did not yield a valid object"
        );
        m.set_feature("f", SubMsFeatureCategory::HotPath, &BTreeMap::new(), "r");
        assert_eq!(m.category_of("f"), Some(SubMsFeatureCategory::HotPath));
    }
}

#[test]
fn only_the_target_feature_changes() {
    let existing = r#"{"lang":"rust","features":{
        "a":{"perf":"hot-path","note":"A keep","p99ByStage":{"op":11}},
        "b":{"perf":"auxiliary","note":"B keep"}
    }}"#;
    let mut m = SubMsFeatureManifest::load_str("rust", existing);
    let mut p99 = BTreeMap::new();
    p99.insert("op".to_string(), 22u64);
    m.set_feature("b", SubMsFeatureCategory::HotPath, &p99, "measured");
    let out = m.to_json();
    assert!(out.contains("\"note\": \"A keep\""));
    assert!(out.contains("\"op\": 11"));
    assert!(out.contains("\"note\": \"B keep\""));
    assert!(out.contains("\"op\": 22"));
    assert_eq!(m.category_of("a"), Some(SubMsFeatureCategory::HotPath));
}

#[test]
fn reclassify_keeps_custom_fields_drops_p99() {
    let existing = r#"{"features":{"f":{"perf":"hot-path","owner":"me","p99ByStage":{"op":5}}}}"#;
    let mut m = SubMsFeatureManifest::load_str("rust", existing);
    m.set_feature(
        "f",
        SubMsFeatureCategory::Structural,
        &BTreeMap::new(),
        "O(n)+",
    );
    let out = m.to_json();
    assert!(out.contains("\"owner\": \"me\""));
    assert!(!out.contains("p99ByStage"));
    assert!(out.contains("\"perf\": \"structural\""));
}

#[test]
fn set_feature_is_idempotent() {
    let mut a = SubMsFeatureManifest::load_str("rust", r#"{"vendor":1,"features":{}}"#);
    let mut b = SubMsFeatureManifest::load_str("rust", r#"{"vendor":1,"features":{}}"#);
    let mut p99 = BTreeMap::new();
    p99.insert("op".to_string(), 7u64);
    a.set_feature("f", SubMsFeatureCategory::HotPath, &p99, "r");
    b.set_feature("f", SubMsFeatureCategory::HotPath, &p99, "r");
    b.set_feature("f", SubMsFeatureCategory::HotPath, &p99, "r");
    assert_eq!(a.to_json(), b.to_json());
}

#[test]
fn degenerate_features_preserves_other_top_level_keys() {
    let existing = r#"{"lang":"rust","vendor":{"keep":true},"features":42}"#;
    let mut m = SubMsFeatureManifest::load_str("rust", existing);
    m.set_feature("f", SubMsFeatureCategory::HotPath, &BTreeMap::new(), "r");
    let out = m.to_json();
    assert!(out.contains("\"keep\": true"));
    assert_eq!(m.category_of("f"), Some(SubMsFeatureCategory::HotPath));
}

#[test]
fn load_missing_then_save_creates_the_file_and_dirs() {
    let dir = std::env::temp_dir().join("subms_feature_mkfile_test");
    let path = dir.join("nested").join("features").join("rust.json");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!path.exists());
    let mut m = SubMsFeatureManifest::load("rust", &path);
    let mut p99 = BTreeMap::new();
    p99.insert("op".to_string(), 7u64);
    m.set_feature("f", SubMsFeatureCategory::HotPath, &p99, "r");
    m.save(&path).unwrap();
    assert!(path.exists());
    let reloaded = SubMsFeatureManifest::load("rust", &path);
    assert_eq!(
        reloaded.category_of("f"),
        Some(SubMsFeatureCategory::HotPath)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------- p99 provenance ----------------

#[test]
fn unstamped_manifest_reads_as_local() {
    // The conservative direction: an unattributed number is withheld, not
    // published as if it came from the conformance box.
    let m = SubMsFeatureManifest::new("rust");
    assert_eq!(m.p99_source(), SubMsP99Source::Local);
    assert_eq!(m.p99_source_ref(), None);
}

#[test]
fn fleet_stamp_records_source_and_instance() {
    let mut m = SubMsFeatureManifest::new("rust");
    m.set_p99_source(SubMsP99Source::Fleet, Some("i-0abc123def456"));
    assert_eq!(m.p99_source(), SubMsP99Source::Fleet);
    assert_eq!(m.p99_source_ref(), Some("i-0abc123def456"));
    let json = m.to_json();
    assert!(json.contains("\"p99_source\": \"fleet\""));
    assert!(json.contains("\"p99_source_ref\": \"i-0abc123def456\""));
}

#[test]
fn local_stamp_clears_a_stale_fleet_reference() {
    // The case that matters: a manifest captured on the box, then re-run on a
    // laptop, must not keep claiming the box.
    let mut m = SubMsFeatureManifest::new("rust");
    m.set_p99_source(SubMsP99Source::Fleet, Some("i-0abc123def456"));
    m.set_p99_source(SubMsP99Source::Local, None);
    assert_eq!(m.p99_source(), SubMsP99Source::Local);
    assert_eq!(m.p99_source_ref(), None);
    assert!(!m.to_json().contains("p99_source_ref"));
}

#[test]
fn a_reference_passed_with_local_is_ignored() {
    let mut m = SubMsFeatureManifest::new("rust");
    m.set_p99_source(SubMsP99Source::Local, Some("i-0abc123def456"));
    assert_eq!(m.p99_source_ref(), None);
}

#[test]
fn an_empty_fleet_reference_is_not_recorded() {
    // An empty instance id is an absent one; writing it would look like
    // provenance while identifying nothing.
    let mut m = SubMsFeatureManifest::new("rust");
    m.set_p99_source(SubMsP99Source::Fleet, Some(""));
    assert_eq!(m.p99_source(), SubMsP99Source::Fleet);
    assert_eq!(m.p99_source_ref(), None);
}

#[test]
fn stamp_round_trips_through_load_str_and_preserves_other_fields() {
    let mut m = SubMsFeatureManifest::new("rust");
    m.set_p99_source(SubMsP99Source::Fleet, Some("i-1"));
    let mut p99 = BTreeMap::new();
    p99.insert("get".to_string(), 900u64);
    m.set_feature("compaction", SubMsFeatureCategory::HotPath, &p99, "flat");
    let reloaded = SubMsFeatureManifest::load_str("rust", &m.to_json());
    assert_eq!(reloaded.p99_source(), SubMsP99Source::Fleet);
    assert_eq!(reloaded.p99_source_ref(), Some("i-1"));
    assert_eq!(
        reloaded.category_of("compaction"),
        Some(SubMsFeatureCategory::HotPath)
    );
}

#[test]
fn an_unknown_source_token_reads_as_local() {
    // A typo withholds numbers instead of publishing them.
    let m = SubMsFeatureManifest::load_str(
        "rust",
        "{\"lang\":\"rust\",\"p99_source\":\"ec2\",\"features\":{}}",
    );
    assert_eq!(m.p99_source(), SubMsP99Source::Local);
    assert_eq!(SubMsP99Source::from_wire("fleet"), SubMsP99Source::Fleet);
    assert_eq!(SubMsP99Source::from_wire("local"), SubMsP99Source::Local);
    assert_eq!(SubMsP99Source::Fleet.as_str(), "fleet");
    assert_eq!(SubMsP99Source::Local.as_str(), "local");
}

#[test]
fn restamping_keeps_the_key_in_place() {
    // set() preserves position, so a re-stamp must not shuffle the document.
    let mut m = SubMsFeatureManifest::new("rust");
    m.set_p99_source(SubMsP99Source::Local, None);
    let first = m.to_json().find("p99_source").unwrap();
    m.set_p99_source(SubMsP99Source::Fleet, Some("i-2"));
    assert_eq!(m.to_json().find("p99_source").unwrap(), first);
}

#[test]
fn env_provenance_defaults_to_local_when_unset() {
    // Serialised with the fleet case below - std::env is process-global, so two
    // tests mutating it in parallel would flake.
    let _guard = env_lock();
    unsafe { std::env::remove_var("SUBMS_FLEET_INSTANCE") };
    assert_eq!(SubMsP99Source::from_env(), (SubMsP99Source::Local, None));
    // A blank value is an unset one, not a nameless fleet box.
    unsafe { std::env::set_var("SUBMS_FLEET_INSTANCE", "   ") };
    assert_eq!(SubMsP99Source::from_env(), (SubMsP99Source::Local, None));
    unsafe { std::env::remove_var("SUBMS_FLEET_INSTANCE") };
}

#[test]
fn env_provenance_reads_the_instance_id() {
    let _guard = env_lock();
    unsafe { std::env::set_var("SUBMS_FLEET_INSTANCE", " i-0abc123 ") };
    assert_eq!(
        SubMsP99Source::from_env(),
        (SubMsP99Source::Fleet, Some("i-0abc123".to_string()))
    );
    unsafe { std::env::remove_var("SUBMS_FLEET_INSTANCE") };
}

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn a_feature_faster_than_base_is_not_reported_as_within_the_delta() {
    // The auxiliary branch fires for anything at or below base, so a feature a
    // third of the baseline was described as "within 10% of base" - a recorded
    // reason that is simply false, on the one field that audits the category.
    let sweep = [(1_024usize, 200u64), (65_536usize, 200u64)];
    let (cat, why) = classify_feature(&sweep, Some(700), None);
    assert_eq!(cat, SubMsFeatureCategory::Auxiliary);
    assert!(why.contains("at or below base"), "{why}");
    assert!(!why.contains("within"), "{why}");
}

#[test]
fn a_feature_just_above_base_still_reads_as_within_the_delta() {
    // 730 is above base 700 but inside the 10% band - the genuine non-effect.
    let sweep = [(1_024usize, 730u64), (65_536usize, 730u64)];
    let (cat, why) = classify_feature(&sweep, Some(700), None);
    assert_eq!(cat, SubMsFeatureCategory::Auxiliary);
    assert!(why.contains("within 10% of base"), "{why}");
}

// ---- v2: the claim line and the undecidable band ----

#[test]
fn a_flat_op_above_the_claim_line_is_reported_not_claimed() {
    // adaptive-radix-tree/serialize measured 30.7 ms flat and was published as a
    // per-op sub-ms claim, because nothing bounded the hot-path branch.
    let sweep = [(4_096usize, 30_000_000u64), (262_144usize, 30_674_448u64)];
    let (cat, why) = classify_feature(&sweep, Some(1_000), None);
    assert_eq!(cat, SubMsFeatureCategory::Reported);
    assert!(why.contains("claim line"), "{why}");
}

#[test]
fn the_claim_line_does_not_swallow_a_genuine_sub_ms_hot_path() {
    let sweep = [(4_096usize, 900_000u64), (262_144usize, 950_000u64)];
    let (cat, _) = classify_feature(&sweep, Some(100), None);
    assert_eq!(cat, SubMsFeatureCategory::HotPath);
}

#[test]
fn a_feature_costing_about_the_guard_is_indeterminate_not_a_coin_toss() {
    // block-cache/metrics, both fleet runs. 269ns vs base 246 read auxiliary and
    // 272ns vs base 245 read hot-path - a 3ns move flipping the category. Both
    // now decline, which is the point: the same answer twice, and an honest one.
    let (a, _) = classify_feature(&[(1usize, 269u64)], Some(246), None);
    let (b, _) = classify_feature(&[(1usize, 272u64)], Some(245), None);
    assert_eq!(a, SubMsFeatureCategory::Indeterminate);
    assert_eq!(b, SubMsFeatureCategory::Indeterminate);
}

#[test]
fn exactly_base_stays_auxiliary_and_is_never_indeterminate() {
    // The band is on the EXCESS, not the guard. Banding the guard instead made
    // this case indeterminate, which is wrong - zero delta is the least
    // ambiguous auxiliary there is.
    let (cat, _) = classify_feature(&[(1usize, 300u64)], Some(300), None);
    assert_eq!(cat, SubMsFeatureCategory::Auxiliary);
}

#[test]
fn a_sweep_straddling_the_structural_guard_is_indeterminate() {
    // 64x N needs 32.5x growth to read structural. ART/serialize measured 39.3x
    // locally and under it on the fleet, splitting across ports on one op.
    let sweep = [(4_096usize, 1_000u64), (262_144usize, 35_000u64)];
    let (cat, why) = classify_feature(&sweep, Some(100), None);
    assert_eq!(cat, SubMsFeatureCategory::Indeterminate);
    assert!(why.contains("too close to call"), "{why}");
}

#[test]
fn a_clearly_superlinear_sweep_is_still_structural() {
    // ART/range-scan on the fleet: 132.9x over 64x N. Nowhere near the band.
    let sweep = [(4_096usize, 540_000u64), (262_144usize, 71_776_000u64)];
    let (cat, _) = classify_feature(&sweep, None, None);
    assert_eq!(cat, SubMsFeatureCategory::Structural);
}

#[test]
fn an_override_still_wins_over_the_claim_line_and_the_band() {
    let sweep = [(1usize, 30_000_000u64)];
    let (cat, why) = classify_feature(&sweep, Some(100), Some(SubMsFeatureCategory::Structural));
    assert_eq!(cat, SubMsFeatureCategory::Structural);
    assert!(why.starts_with("override:"), "{why}");
}

#[test]
fn every_category_round_trips_through_its_wire_value() {
    for c in [
        SubMsFeatureCategory::HotPath,
        SubMsFeatureCategory::Structural,
        SubMsFeatureCategory::Auxiliary,
        SubMsFeatureCategory::Reported,
        SubMsFeatureCategory::Indeterminate,
    ] {
        assert_eq!(SubMsFeatureCategory::from_wire(c.as_str()), Some(c));
    }
}

// ---- v2: per-feature provenance ----

#[test]
fn a_feature_written_this_run_carries_this_runs_provenance() {
    let mut m = SubMsFeatureManifest::new("rust");
    m.set_p99_source(SubMsP99Source::Fleet, Some("i-07f269c7f5d290fc4"));
    let mut p99 = BTreeMap::new();
    p99.insert("get".to_string(), 300u64);
    m.set_feature("counting", SubMsFeatureCategory::HotPath, &p99, "why");
    assert_eq!(
        m.feature_p99_source("counting"),
        Some(SubMsP99Source::Fleet)
    );
    let json = m.to_json();
    assert!(json.contains("\"p99Source\""), "{json}");
    assert!(json.contains("i-07f269c7f5d290fc4"), "{json}");
}

#[test]
fn a_feature_the_run_did_not_touch_keeps_its_own_provenance() {
    // The bug this exists for: the manifest MERGE-writes, so a feature that is
    // not re-measured keeps its old numbers. Under a file-level stamp alone it
    // silently inherited the new one - a local figure inside a `fleet` file.
    let mut first = SubMsFeatureManifest::new("rust");
    first.set_p99_source(SubMsP99Source::Local, None);
    let mut p99 = BTreeMap::new();
    p99.insert("op".to_string(), 100u64);
    first.set_feature("serde", SubMsFeatureCategory::Auxiliary, &p99, "local run");
    let carried = first.to_json();

    let mut second = SubMsFeatureManifest::load_str("rust", &carried);
    second.set_p99_source(SubMsP99Source::Fleet, Some("i-abc123ff"));
    second.set_feature("counting", SubMsFeatureCategory::HotPath, &p99, "fleet run");

    assert_eq!(
        second.feature_p99_source("counting"),
        Some(SubMsP99Source::Fleet)
    );
    assert_eq!(
        second.feature_p99_source("serde"),
        Some(SubMsP99Source::Local),
        "a carried-over feature must NOT inherit the fleet stamp"
    );
}

#[test]
fn a_pre_v2_manifest_falls_back_to_the_file_level_stamp() {
    let legacy = r#"{"lang":"rust","p99_source":"fleet","features":{"x":{"perf":"hot-path"}}}"#;
    let m = SubMsFeatureManifest::load_str("rust", legacy);
    assert_eq!(m.feature_p99_source("x"), Some(SubMsP99Source::Fleet));
}

#[test]
fn re_running_a_feature_locally_clears_its_fleet_reference() {
    let mut m = SubMsFeatureManifest::new("rust");
    m.set_p99_source(SubMsP99Source::Fleet, Some("i-abc123ff"));
    let p99 = BTreeMap::new();
    m.set_feature("x", SubMsFeatureCategory::Auxiliary, &p99, "fleet");
    m.set_p99_source(SubMsP99Source::Local, None);
    m.set_feature("x", SubMsFeatureCategory::Auxiliary, &p99, "local");
    assert_eq!(m.feature_p99_source("x"), Some(SubMsP99Source::Local));
    assert!(
        !m.to_json().contains("i-abc123ff"),
        "stale fleet ref survived a local re-run"
    );
}

// ---- v2: per-stage classification ----

fn stage(p99: u64, cat: SubMsFeatureCategory) -> SubMsStageClass {
    SubMsStageClass {
        p99_ns: p99,
        category: cat,
        reason: cat.as_str().to_string(),
    }
}

#[test]
fn a_mixed_feature_rolls_up_to_its_most_restrictive_stage() {
    // adaptive-radix-tree/concurrent-reads: a 1227ns get and a 44ms snapshot,
    // published under ONE hot-path label. The get is genuinely hot; the snapshot
    // is not, and the rollup must not let it hide.
    let mut stages = BTreeMap::new();
    stages.insert(
        "get".to_string(),
        stage(1_227, SubMsFeatureCategory::HotPath),
    );
    stages.insert(
        "snapshot".to_string(),
        stage(44_037_700, SubMsFeatureCategory::Reported),
    );
    assert_eq!(roll_up_stages(&stages), SubMsFeatureCategory::Reported);
}

#[test]
fn an_all_hot_feature_still_rolls_up_hot() {
    let mut stages = BTreeMap::new();
    stages.insert("add".to_string(), stage(60, SubMsFeatureCategory::HotPath));
    stages.insert(
        "contains".to_string(),
        stage(58, SubMsFeatureCategory::HotPath),
    );
    assert_eq!(roll_up_stages(&stages), SubMsFeatureCategory::HotPath);
}

#[test]
fn auxiliary_stages_never_drag_a_hot_feature_down() {
    let mut stages = BTreeMap::new();
    stages.insert(
        "probe".to_string(),
        stage(60, SubMsFeatureCategory::HotPath),
    );
    stages.insert(
        "stats".to_string(),
        stage(10, SubMsFeatureCategory::Auxiliary),
    );
    assert_eq!(roll_up_stages(&stages), SubMsFeatureCategory::HotPath);
}

#[test]
fn one_indeterminate_stage_makes_the_feature_indeterminate() {
    // Not knowing is more restrictive than knowing it is excluded: a summary
    // that reads `structural` would imply the whole feature was measured.
    let mut stages = BTreeMap::new();
    stages.insert(
        "a".to_string(),
        stage(100, SubMsFeatureCategory::Structural),
    );
    stages.insert(
        "b".to_string(),
        stage(100, SubMsFeatureCategory::Indeterminate),
    );
    assert_eq!(roll_up_stages(&stages), SubMsFeatureCategory::Indeterminate);
}

#[test]
fn set_feature_stages_writes_per_stage_detail_and_the_rollup() {
    let mut m = SubMsFeatureManifest::new("rust");
    m.set_p99_source(SubMsP99Source::Fleet, Some("i-abc123ff"));
    let mut stages = BTreeMap::new();
    stages.insert(
        "get".to_string(),
        stage(1_227, SubMsFeatureCategory::HotPath),
    );
    stages.insert(
        "snapshot".to_string(),
        stage(44_037_700, SubMsFeatureCategory::Reported),
    );
    m.set_feature_stages("concurrent-reads", &stages, "mixed");

    assert_eq!(
        m.stage_category("concurrent-reads", "get"),
        Some(SubMsFeatureCategory::HotPath)
    );
    assert_eq!(
        m.stage_category("concurrent-reads", "snapshot"),
        Some(SubMsFeatureCategory::Reported)
    );
    assert_eq!(
        m.feature_p99_source("concurrent-reads"),
        Some(SubMsP99Source::Fleet)
    );
    let json = m.to_json();
    // The rollup is what a summary-only consumer sees, and it must be the
    // restrictive one.
    assert!(json.contains("\"perf\": \"reported\""), "{json}");
    // The flat shape stays populated so an older reader is not left blank.
    assert!(json.contains("p99ByStage"), "{json}");
}

#[test]
fn an_empty_stage_set_rolls_up_to_auxiliary() {
    let stages: BTreeMap<String, SubMsStageClass> = BTreeMap::new();
    assert_eq!(roll_up_stages(&stages), SubMsFeatureCategory::Auxiliary);
}
