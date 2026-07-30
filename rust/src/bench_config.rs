//! Per-recipe bench configuration - the typed view of a recipe's
//! `.subms/perf/controls.json`.
//!
//! Like [`crate::SubMsFeatureManifest`], it loads, merge-updates, and saves
//! through a hand-written zero-dependency JSON value model that PRESERVES every
//! field this harness does not own (the fleet orchestrator's `sample_cap` /
//! `rounds` / `entries`, a third party's custom keys) across a round-trip - a
//! setter touches only the key it names.
//!
//! One semantic default worth stating: an ABSENT `cpu_pin` means
//! [`SubMsCpuPin::Single`]. The historical behaviour is to pin a single-threaded
//! recipe to one isolated core for a stable p99; a multi-threaded recipe (its own
//! writer plus a worker thread) sets `"multi"` (with `cores`) or `"none"` so a
//! single-core pin does not starve it.

use std::fs;
use std::io;
use std::path::Path;

use crate::feature::{Json, parse_json};

/// How the bench harness should place a recipe's process on the box's CPUs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SubMsCpuPin {
    /// Pin to ONE isolated core - a stable single-threaded p99. The default for
    /// a single-threaded recipe; the box's `isolcpus` reserves the core.
    Single,
    /// Pin to `cores` cores. For a MULTI-threaded recipe (its own writer + a
    /// worker) that wants dedicated cores rather than starving on one.
    Multi,
    /// No pinning - run across all cores on the general scheduler. For a
    /// multi-threaded recipe where per-core isolation is not available/needed.
    None,
}

impl SubMsCpuPin {
    /// The lowercase wire token written to `.subms/perf/controls.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            SubMsCpuPin::Single => "single",
            SubMsCpuPin::Multi => "multi",
            SubMsCpuPin::None => "none",
        }
    }

    /// Parse a wire token. Also tolerates the legacy boolean form (`true` ->
    /// `Single`, `false` -> `None`). Anything unrecognised -> `None` result so the
    /// caller can apply its own default.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "single" | "one" | "true" => Some(SubMsCpuPin::Single),
            "multi" | "cores" => Some(SubMsCpuPin::Multi),
            "none" | "off" | "false" => Some(SubMsCpuPin::None),
            _ => None,
        }
    }
}

/// The typed, load/merge-save model of `.subms/perf/controls.json`.
pub struct SubMsBenchConfig {
    /// Always a `Json::Obj`; foreign keys ride along untouched.
    root: Json,
}

impl Default for SubMsBenchConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl SubMsBenchConfig {
    /// An empty config (every accessor returns its documented default).
    pub fn new() -> Self {
        Self {
            root: Json::Obj(Vec::new()),
        }
    }

    /// Parse from JSON text. A missing, empty, malformed, or non-object input
    /// yields an empty config rather than panicking - the accessors fall back to
    /// their defaults, so a broken controls.json never breaks a bench.
    pub fn load_str(text: &str) -> Self {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Self::new();
        }
        match parse_json(trimmed) {
            Ok(root @ Json::Obj(_)) => Self { root },
            _ => Self::new(),
        }
    }

    /// Load from a file; a missing file is not an error (returns an empty config).
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(Self::load_str(&text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::new()),
            Err(e) => Err(e),
        }
    }

    /// Serialise to pretty JSON (stable insertion-order keys).
    pub fn to_json(&self) -> String {
        self.root.to_pretty()
    }

    /// Write to `path`, creating parent directories if needed.
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, self.to_json())
    }

    // ---- typed accessors (absent -> documented default) ----

    /// How the bench harness should place this recipe on the box's CPUs. ABSENT
    /// (or unrecognised) defaults to [`SubMsCpuPin::Single`] - a single-threaded
    /// recipe wants one isolated core for a stable p99. Tolerates the legacy
    /// boolean form (`true` -> `Single`, `false` -> `None`).
    pub fn cpu_pin(&self) -> SubMsCpuPin {
        match self.get("cpu_pin") {
            Some(Json::Str(s)) => SubMsCpuPin::from_wire(s).unwrap_or(SubMsCpuPin::Single),
            Some(Json::Bool(true)) => SubMsCpuPin::Single,
            Some(Json::Bool(false)) => SubMsCpuPin::None,
            _ => SubMsCpuPin::Single,
        }
    }

    /// How many cores a [`SubMsCpuPin::Multi`] recipe wants pinned. `None` =
    /// unspecified (the harness leaves core allocation to the scheduler).
    pub fn cores(&self) -> Option<u64> {
        self.get_u64("cores")
    }

    /// The per-op sample cap the capture should raise the harness to. `None` = the
    /// harness default.
    pub fn sample_cap(&self) -> Option<u64> {
        self.get_u64("sample_cap")
    }

    /// Free-text note on why the config is set the way it is.
    pub fn reason(&self) -> Option<&str> {
        match self.get("reason") {
            Some(Json::Str(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    // ---- setters (merge-preserving; touch only the named key) ----

    pub fn set_cpu_pin(&mut self, mode: SubMsCpuPin) -> &mut Self {
        self.set("cpu_pin", Json::Str(mode.as_str().to_string()));
        self
    }

    pub fn set_cores(&mut self, cores: u64) -> &mut Self {
        self.set("cores", Json::Num(cores.to_string()));
        self
    }

    pub fn set_sample_cap(&mut self, cap: u64) -> &mut Self {
        self.set("sample_cap", Json::Num(cap.to_string()));
        self
    }

    pub fn set_reason(&mut self, reason: &str) -> &mut Self {
        self.set("reason", Json::Str(reason.to_string()));
        self
    }

    // ---- internals ----

    fn get(&self, key: &str) -> Option<&Json> {
        match &self.root {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn get_u64(&self, key: &str) -> Option<u64> {
        match self.get(key) {
            Some(Json::Num(n)) => n.trim().parse::<u64>().ok(),
            _ => None,
        }
    }

    fn set(&mut self, key: &str, val: Json) {
        if let Some(obj) = self.root.as_object_mut() {
            if let Some(slot) = obj.iter_mut().find(|(k, _)| k == key) {
                slot.1 = val;
            } else {
                obj.push((key.to_string(), val));
            }
        }
    }
}

#[cfg(test)]
#[path = "bench_config_tests.rs"]
mod tests;
