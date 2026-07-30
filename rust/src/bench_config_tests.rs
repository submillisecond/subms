use super::*;

#[test]
fn new_defaults_cpu_pin_single() {
    let c = SubMsBenchConfig::new();
    assert_eq!(c.cpu_pin(), SubMsCpuPin::Single, "absent cpu_pin -> Single");
    assert_eq!(c.cores(), None);
    assert_eq!(c.sample_cap(), None);
    assert_eq!(c.reason(), None);
}

#[test]
fn default_impl_matches_new() {
    let d = SubMsBenchConfig::default();
    assert_eq!(d.cpu_pin(), SubMsCpuPin::Single);
    assert_eq!(d.to_json(), SubMsBenchConfig::new().to_json());
}

#[test]
fn cpu_pin_wire_tokens_round_trip() {
    for (tok, want) in [
        ("single", SubMsCpuPin::Single),
        ("multi", SubMsCpuPin::Multi),
        ("none", SubMsCpuPin::None),
    ] {
        assert_eq!(SubMsCpuPin::from_wire(tok), Some(want));
        assert_eq!(want.as_str(), tok);
    }
    // aliases + legacy boolean words map onto the canonical set.
    assert_eq!(SubMsCpuPin::from_wire("ONE"), Some(SubMsCpuPin::Single));
    assert_eq!(SubMsCpuPin::from_wire("true"), Some(SubMsCpuPin::Single));
    assert_eq!(SubMsCpuPin::from_wire("false"), Some(SubMsCpuPin::None));
    assert_eq!(SubMsCpuPin::from_wire("off"), Some(SubMsCpuPin::None));
    assert_eq!(SubMsCpuPin::from_wire("garbage"), None);
}

#[test]
fn load_str_reads_typed_fields() {
    let c = SubMsBenchConfig::load_str(
        r#"{ "cpu_pin": "multi", "cores": 2, "sample_cap": 50000, "reason": "multi-threaded" }"#,
    );
    assert_eq!(c.cpu_pin(), SubMsCpuPin::Multi);
    assert_eq!(c.cores(), Some(2));
    assert_eq!(c.sample_cap(), Some(50000));
    assert_eq!(c.reason(), Some("multi-threaded"));
}

#[test]
fn legacy_boolean_cpu_pin_still_reads() {
    // controls.json written before the enum landed used a bare boolean.
    assert_eq!(
        SubMsBenchConfig::load_str(r#"{ "cpu_pin": true }"#).cpu_pin(),
        SubMsCpuPin::Single
    );
    assert_eq!(
        SubMsBenchConfig::load_str(r#"{ "cpu_pin": false }"#).cpu_pin(),
        SubMsCpuPin::None
    );
}

#[test]
fn absent_cpu_pin_defaults_single_even_with_other_keys() {
    let c = SubMsBenchConfig::load_str(r#"{ "sample_cap": 500 }"#);
    assert_eq!(c.cpu_pin(), SubMsCpuPin::Single);
    assert_eq!(c.sample_cap(), Some(500));
}

#[test]
fn malformed_input_yields_empty_never_panics() {
    for bad in [
        "",
        "   ",
        "not json",
        "{",
        "[1,2,3]", // valid json but not an object
        "42",
        "null",
        "\"a string\"",
        "{\"cpu_pin\":",
    ] {
        let c = SubMsBenchConfig::load_str(bad);
        // falls back to defaults, stays usable
        assert_eq!(
            c.cpu_pin(),
            SubMsCpuPin::Single,
            "input {bad:?} should default cpu_pin=Single"
        );
        assert_eq!(c.cores(), None);
    }
}

#[test]
fn wrong_typed_values_fall_back_to_default() {
    // cpu_pin as an unknown string, cores as a float string, sample_cap as bool.
    let c = SubMsBenchConfig::load_str(r#"{ "cpu_pin": "yes", "cores": 2.5, "sample_cap": true }"#);
    assert_eq!(
        c.cpu_pin(),
        SubMsCpuPin::Single,
        "unrecognised cpu_pin -> default Single"
    );
    assert_eq!(c.cores(), None, "non-integer cores -> None");
    assert_eq!(c.sample_cap(), None, "non-integer sample_cap -> None");
}

#[test]
fn merge_preserves_foreign_fields() {
    // A controls.json the harness does not fully own: orchestrator + 3rd-party keys.
    let src = r#"{
  "sample_cap": 50000,
  "rounds": 50,
  "storage": { "rounds": 80, "ops_per_round": 2000 },
  "vendor": { "team": "risk", "ticket": "PERF-9" },
  "cpu_pin": "single"
}"#;
    let mut c = SubMsBenchConfig::load_str(src);
    // touch only cpu_pin + cores.
    c.set_cpu_pin(SubMsCpuPin::Multi).set_cores(4);
    let out = c.to_json();
    // every foreign field survives, plus the updated ones.
    for needle in [
        "\"rounds\"",
        "\"storage\"",
        "ops_per_round",
        "\"vendor\"",
        "PERF-9",
        "50000",
    ] {
        assert!(
            out.contains(needle),
            "foreign field {needle} lost in round-trip"
        );
    }
    let reloaded = SubMsBenchConfig::load_str(&out);
    assert_eq!(reloaded.cpu_pin(), SubMsCpuPin::Multi);
    assert_eq!(reloaded.cores(), Some(4));
    assert_eq!(reloaded.sample_cap(), Some(50000));
}

#[test]
fn setters_are_idempotent_and_positional() {
    let mut c = SubMsBenchConfig::load_str(r#"{ "cpu_pin": "single", "cores": 1 }"#);
    c.set_cpu_pin(SubMsCpuPin::None);
    c.set_cpu_pin(SubMsCpuPin::None); // second set must not duplicate the key
    c.set_cores(8).set_sample_cap(50000).set_reason("r");
    let out = c.to_json();
    assert_eq!(out.matches("\"cpu_pin\"").count(), 1, "cpu_pin duplicated");
    assert_eq!(out.matches("\"cores\"").count(), 1);
    let r = SubMsBenchConfig::load_str(&out);
    assert_eq!(r.cpu_pin(), SubMsCpuPin::None);
    assert_eq!(r.cores(), Some(8));
    assert_eq!(r.sample_cap(), Some(50000));
    assert_eq!(r.reason(), Some("r"));
}

#[test]
fn load_missing_then_save_creates_file_and_dirs() {
    let dir = std::env::temp_dir().join(format!(
        "subms-benchcfg-{}-{}",
        std::process::id(),
        "load-missing"
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir
        .join("nested")
        .join(".subms")
        .join("perf")
        .join("controls.json");
    assert!(!path.exists());
    let c = SubMsBenchConfig::load(&path).unwrap();
    assert_eq!(c.cpu_pin(), SubMsCpuPin::Single, "missing file -> default");

    let mut c = c;
    c.set_cpu_pin(SubMsCpuPin::Multi).set_cores(2);
    c.save(&path).unwrap();
    assert!(path.exists());
    let reloaded = SubMsBenchConfig::load(&path).unwrap();
    assert_eq!(reloaded.cpu_pin(), SubMsCpuPin::Multi);
    assert_eq!(reloaded.cores(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}
