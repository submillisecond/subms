//! Tests for `SubMsAppEnv`, `SubMsAppRegion`, and the small `env_*` utility
//! surface. The deterministic logic lives in `parse(&str)` and the bool/numeric
//! parsers; those are tested exhaustively. The `from_env()` shape is a
//! two-line wrapper around `env_str` + `parse` so it's covered indirectly
//! through one end-to-end smoke test per enum that mutates a uniquely-named
//! env var. Each env-touching test uses a distinct key so parallel test runs
//! don't race.

use crate::{SubMsAppEnv, SubMsAppRegion, env_bool, env_f64, env_i64, env_or, env_str, env_u64};
use std::env;

// ---------- SubMsAppEnv::parse ----------

#[test]
fn app_env_parses_canonical_lowercase() {
    assert_eq!(SubMsAppEnv::parse("local"), SubMsAppEnv::Local);
    assert_eq!(SubMsAppEnv::parse("dev"), SubMsAppEnv::Dev);
    assert_eq!(SubMsAppEnv::parse("uat"), SubMsAppEnv::Uat);
    assert_eq!(SubMsAppEnv::parse("prod"), SubMsAppEnv::Prod);
}

#[test]
fn app_env_parses_mixed_case() {
    assert_eq!(SubMsAppEnv::parse("LOCAL"), SubMsAppEnv::Local);
    assert_eq!(SubMsAppEnv::parse("Dev"), SubMsAppEnv::Dev);
    assert_eq!(SubMsAppEnv::parse("PROD"), SubMsAppEnv::Prod);
    assert_eq!(SubMsAppEnv::parse("uAt"), SubMsAppEnv::Uat);
}

#[test]
fn app_env_parses_synonyms() {
    assert_eq!(SubMsAppEnv::parse("development"), SubMsAppEnv::Dev);
    assert_eq!(SubMsAppEnv::parse("production"), SubMsAppEnv::Prod);
    assert_eq!(SubMsAppEnv::parse("staging"), SubMsAppEnv::Uat);
    assert_eq!(SubMsAppEnv::parse("stage"), SubMsAppEnv::Uat);
}

#[test]
fn app_env_trims_whitespace() {
    assert_eq!(SubMsAppEnv::parse("  prod  "), SubMsAppEnv::Prod);
    assert_eq!(SubMsAppEnv::parse("\tdev\n"), SubMsAppEnv::Dev);
}

#[test]
fn app_env_unknown_falls_back_to_local() {
    assert_eq!(SubMsAppEnv::parse(""), SubMsAppEnv::Local);
    assert_eq!(SubMsAppEnv::parse("nonsense"), SubMsAppEnv::Local);
    assert_eq!(SubMsAppEnv::parse("preprod"), SubMsAppEnv::Local);
}

#[test]
fn app_env_default_is_local() {
    assert_eq!(SubMsAppEnv::default(), SubMsAppEnv::Local);
}

#[test]
fn app_env_as_str_and_display_match() {
    assert_eq!(SubMsAppEnv::Local.as_str(), "local");
    assert_eq!(SubMsAppEnv::Prod.as_str(), "prod");
    assert_eq!(format!("{}", SubMsAppEnv::Uat), "uat");
    assert_eq!(format!("{}", SubMsAppEnv::Dev), "dev");
}

#[test]
fn app_env_from_env_reads_app_env_var() {
    let key = "APP_ENV";
    let prior = env::var(key).ok();
    unsafe {
        env::set_var(key, "prod");
    }
    let got = SubMsAppEnv::from_env();
    match prior {
        Some(v) => unsafe { env::set_var(key, v) },
        None => unsafe { env::remove_var(key) },
    }
    assert_eq!(got, SubMsAppEnv::Prod);
}

// ---------- SubMsAppRegion::parse ----------

#[test]
fn app_region_parses_canonical() {
    assert_eq!(SubMsAppRegion::parse("na"), SubMsAppRegion::Na);
    assert_eq!(SubMsAppRegion::parse("latam"), SubMsAppRegion::Latam);
    assert_eq!(SubMsAppRegion::parse("emea"), SubMsAppRegion::Emea);
    assert_eq!(SubMsAppRegion::parse("apac"), SubMsAppRegion::Apac);
}

#[test]
fn app_region_parses_mixed_case() {
    assert_eq!(SubMsAppRegion::parse("NA"), SubMsAppRegion::Na);
    assert_eq!(SubMsAppRegion::parse("EMEA"), SubMsAppRegion::Emea);
    assert_eq!(SubMsAppRegion::parse("ApAc"), SubMsAppRegion::Apac);
}

#[test]
fn app_region_parses_synonyms() {
    assert_eq!(SubMsAppRegion::parse("north-america"), SubMsAppRegion::Na);
    assert_eq!(SubMsAppRegion::parse("north_america"), SubMsAppRegion::Na);
    assert_eq!(SubMsAppRegion::parse("namer"), SubMsAppRegion::Na);
    assert_eq!(SubMsAppRegion::parse("lat-am"), SubMsAppRegion::Latam);
    assert_eq!(
        SubMsAppRegion::parse("latin-america"),
        SubMsAppRegion::Latam
    );
    assert_eq!(SubMsAppRegion::parse("asia-pacific"), SubMsAppRegion::Apac);
}

#[test]
fn app_region_unknown_falls_back_to_unknown() {
    assert_eq!(SubMsAppRegion::parse(""), SubMsAppRegion::Unknown);
    assert_eq!(SubMsAppRegion::parse("moon"), SubMsAppRegion::Unknown);
    assert_eq!(SubMsAppRegion::parse("antarctica"), SubMsAppRegion::Unknown);
}

#[test]
fn app_region_default_is_unknown() {
    assert_eq!(SubMsAppRegion::default(), SubMsAppRegion::Unknown);
}

#[test]
fn app_region_as_str_and_display_match() {
    assert_eq!(SubMsAppRegion::Na.as_str(), "na");
    assert_eq!(SubMsAppRegion::Latam.as_str(), "latam");
    assert_eq!(format!("{}", SubMsAppRegion::Emea), "emea");
    assert_eq!(format!("{}", SubMsAppRegion::Unknown), "unknown");
}

#[test]
fn app_region_from_env_reads_app_region_var() {
    let key = "APP_REGION";
    let prior = env::var(key).ok();
    unsafe {
        env::set_var(key, "emea");
    }
    let got = SubMsAppRegion::from_env();
    match prior {
        Some(v) => unsafe { env::set_var(key, v) },
        None => unsafe { env::remove_var(key) },
    }
    assert_eq!(got, SubMsAppRegion::Emea);
}

// ---------- env_* utilities ----------

#[test]
fn env_str_returns_none_for_unset() {
    assert!(env_str("SUBMS_TEST_NEVER_SET_8a3f").is_none());
}

#[test]
fn env_str_treats_empty_as_absent() {
    let key = "SUBMS_TEST_EMPTY_4c91";
    unsafe {
        env::set_var(key, "");
    }
    assert!(env_str(key).is_none());
    unsafe {
        env::remove_var(key);
    }
}

#[test]
fn env_str_reads_set_value() {
    let key = "SUBMS_TEST_STR_d2b7";
    unsafe {
        env::set_var(key, "hello");
    }
    assert_eq!(env_str(key).as_deref(), Some("hello"));
    unsafe {
        env::remove_var(key);
    }
}

#[test]
fn env_or_falls_back_when_unset() {
    assert_eq!(env_or("SUBMS_TEST_OR_MISS_9e1c", "fallback"), "fallback");
}

#[test]
fn env_or_uses_value_when_set() {
    let key = "SUBMS_TEST_OR_HIT_7b22";
    unsafe {
        env::set_var(key, "set");
    }
    assert_eq!(env_or(key, "fallback"), "set");
    unsafe {
        env::remove_var(key);
    }
}

#[test]
fn env_bool_parses_truthy_variants() {
    for v in ["true", "TRUE", "1", "yes", "on", " ON "] {
        let key = format!("SUBMS_TEST_BOOL_T_{:p}", v);
        unsafe {
            env::set_var(&key, v);
        }
        assert!(env_bool(&key, false), "expected true for {v:?}");
        unsafe {
            env::remove_var(&key);
        }
    }
}

#[test]
fn env_bool_parses_falsy_variants() {
    for v in ["false", "FALSE", "0", "no", "off"] {
        let key = format!("SUBMS_TEST_BOOL_F_{:p}", v);
        unsafe {
            env::set_var(&key, v);
        }
        assert!(!env_bool(&key, true), "expected false for {v:?}");
        unsafe {
            env::remove_var(&key);
        }
    }
}

#[test]
fn env_bool_falls_back_on_garbage() {
    let key = "SUBMS_TEST_BOOL_GARBAGE_e88a";
    unsafe {
        env::set_var(key, "maybe");
    }
    assert!(env_bool(key, true));
    assert!(!env_bool(key, false));
    unsafe {
        env::remove_var(key);
    }
}

#[test]
fn env_i64_parses_signed_int() {
    let key = "SUBMS_TEST_I64_a0bc";
    unsafe {
        env::set_var(key, "-12345");
    }
    assert_eq!(env_i64(key, 0), -12345);
    unsafe {
        env::remove_var(key);
    }
}

#[test]
fn env_i64_falls_back_on_unparseable() {
    let key = "SUBMS_TEST_I64_BAD_3331";
    unsafe {
        env::set_var(key, "twelve");
    }
    assert_eq!(env_i64(key, 7), 7);
    unsafe {
        env::remove_var(key);
    }
}

#[test]
fn env_u64_and_f64_smoke() {
    let ku = "SUBMS_TEST_U64_d104";
    let kf = "SUBMS_TEST_F64_d104";
    unsafe {
        env::set_var(ku, "9000");
        env::set_var(kf, "1.5e-3");
    }
    assert_eq!(env_u64(ku, 0), 9000);
    assert_eq!(env_f64(kf, 0.0), 1.5e-3);
    unsafe {
        env::remove_var(ku);
        env::remove_var(kf);
    }
}
