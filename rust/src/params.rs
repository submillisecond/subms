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
    /// Reads `entries` / `warmup` / `seed` / `sample_cap`, defaulting any missing key.
    pub fn from_map(args: &BTreeMap<String, String>) -> Self {
        let d = Self::default();
        Self {
            entries: parse_usize(args, "entries", d.entries),
            warmup: parse_usize(args, "warmup", d.warmup),
            seed: parse_u64(args, "seed", d.seed),
            sample_cap: parse_usize(args, "sample_cap", d.sample_cap),
        }
    }

    /// Equivalent to `from_map(&read_stdin_kv())`.
    pub fn from_stdin() -> Self {
        Self::from_map(&crate::read_stdin_kv())
    }
}

#[cfg(test)]
#[path = "params_tests.rs"]
mod tests;
