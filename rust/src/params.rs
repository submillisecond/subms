//! Stdin param parsing helpers used by `SubMsRecipe` implementations.

use std::collections::BTreeMap;

use crate::SubMsBenchParams;

/// Parse a `usize`; falls back to `default` if missing or unparseable.
pub fn parse_usize(args: &BTreeMap<String, String>, key: &str, default: usize) -> usize {
    args.get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Parse a `u64`; falls back to `default` if missing or unparseable.
pub fn parse_u64(args: &BTreeMap<String, String>, key: &str, default: u64) -> u64 {
    args.get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Read a string; falls back to `default` if absent.
pub fn parse_string(args: &BTreeMap<String, String>, key: &str, default: &str) -> String {
    args.get(key)
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

/// Accepts `1`/`true`/`on`/`yes` (and `0`/`false`/`off`/`no`) case-insensitively.
/// Unknown or missing -> `default`.
pub fn parse_bool(args: &BTreeMap<String, String>, key: &str, default: bool) -> bool {
    match args.get(key).map(|s| s.to_ascii_lowercase()) {
        Some(s) if matches!(s.as_str(), "1" | "true" | "on" | "yes") => true,
        Some(s) if matches!(s.as_str(), "0" | "false" | "off" | "no") => false,
        _ => default,
    }
}

impl SubMsBenchParams {
    /// Reads `entries` / `warmup` / `seed`, defaulting any missing key.
    pub fn from_map(args: &BTreeMap<String, String>) -> Self {
        let d = Self::default();
        Self {
            entries: parse_usize(args, "entries", d.entries),
            warmup: parse_usize(args, "warmup", d.warmup),
            seed: parse_u64(args, "seed", d.seed),
        }
    }

    /// Equivalent to `from_map(&read_stdin_kv())`.
    pub fn from_stdin() -> Self {
        Self::from_map(&crate::read_stdin_kv())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m<const N: usize>(pairs: [(&str, &str); N]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn parse_usize_present() {
        assert_eq!(parse_usize(&m([("n", "1234")]), "n", 0), 1234);
    }
    #[test]
    fn parse_usize_missing() {
        assert_eq!(parse_usize(&m([("other", "1")]), "n", 42), 42);
    }
    #[test]
    fn parse_usize_invalid_falls_back() {
        assert_eq!(parse_usize(&m([("n", "abc")]), "n", 42), 42);
    }

    #[test]
    fn parse_u64_present() {
        assert_eq!(parse_u64(&m([("seed", "99")]), "seed", 0), 99);
    }
    #[test]
    fn parse_u64_missing() {
        assert_eq!(parse_u64(&m([]), "seed", 7), 7);
    }
    #[test]
    fn parse_u64_invalid_falls_back() {
        assert_eq!(parse_u64(&m([("seed", "not-a-number")]), "seed", 7), 7);
    }

    #[test]
    fn parse_string_present() {
        assert_eq!(parse_string(&m([("mode", "on")]), "mode", "off"), "on");
    }
    #[test]
    fn parse_string_missing() {
        assert_eq!(parse_string(&m([]), "mode", "off"), "off");
    }

    #[test]
    fn parse_bool_accepts_truthy() {
        for v in ["1", "true", "TRUE", "on", "On", "yes", "Yes"] {
            assert!(parse_bool(&m([("x", v)]), "x", false), "{v} should be true");
        }
    }
    #[test]
    fn parse_bool_accepts_falsy() {
        for v in ["0", "false", "off", "no", "NO"] {
            assert!(
                !parse_bool(&m([("x", v)]), "x", true),
                "{v} should be false"
            );
        }
    }
    #[test]
    fn parse_bool_unknown_falls_back() {
        assert!(parse_bool(&m([("x", "maybe")]), "x", true));
        assert!(!parse_bool(&m([("x", "maybe")]), "x", false));
    }
    #[test]
    fn parse_bool_missing_falls_back() {
        assert!(parse_bool(&m([]), "x", true));
        assert!(!parse_bool(&m([]), "x", false));
    }

    #[test]
    fn bench_params_from_map_uses_defaults_when_missing() {
        let p = SubMsBenchParams::from_map(&m([]));
        let d = SubMsBenchParams::default();
        assert_eq!(p.entries, d.entries);
        assert_eq!(p.warmup, d.warmup);
        assert_eq!(p.seed, d.seed);
    }

    #[test]
    fn bench_params_from_map_reads_overrides() {
        let p = SubMsBenchParams::from_map(&m([
            ("entries", "1000"),
            ("warmup", "100"),
            ("seed", "42"),
        ]));
        assert_eq!(p.entries, 1000);
        assert_eq!(p.warmup, 100);
        assert_eq!(p.seed, 42);
    }

    #[test]
    fn bench_params_from_map_ignores_garbage_values() {
        let p =
            SubMsBenchParams::from_map(&m([("entries", "abc"), ("warmup", "?"), ("seed", "nope")]));
        let d = SubMsBenchParams::default();
        assert_eq!(p.entries, d.entries);
        assert_eq!(p.warmup, d.warmup);
        assert_eq!(p.seed, d.seed);
    }
}
