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
    let cases: [(&str, Vec<(usize, u64)>, SubMsFeatureCategory); 5] = [
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
