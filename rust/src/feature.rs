//! Per-feature latency classification + manifest.
//!
//! A recipe's optional features are not all the same kind of thing: some change
//! the per-op hot path (a measured p99 claim), some are O(n) whole-structure ops
//! (serialize, compaction) that cannot honestly carry a per-op sub-ms number, and
//! some are pure capabilities with no latency delta (serde derives). This module
//! lets the *bench decide the category* from a size sweep, and merge-writes the
//! decision into a per-language manifest `.subms/features/<lang>.json` that the
//! website renders.
//!
//! The manifest is a public contract: the merge preserves any fields the harness
//! does not own (yours, or another tool's), touching only a feature's `perf`
//! rating + `p99ByStage`. Zero-dependency: the JSON value model, parser and
//! serialiser here are hand-written, like the rest of the crate.
//!
//! ```
//! use subms::{SubMsFeatureCategory, classify_feature};
//!
//! // A flat p99 across a 64x size sweep, well above the base op -> hot-path.
//! let sweep = [(1_024usize, 300u64), (65_536usize, 320u64)];
//! let (cat, _why) = classify_feature(&sweep, Some(50), None);
//! assert_eq!(cat, SubMsFeatureCategory::HotPath);
//!
//! // p99 grows ~linearly with size -> structural (O(n)).
//! let sweep = [(1_024usize, 1_000u64), (65_536usize, 64_000u64)];
//! let (cat, _why) = classify_feature(&sweep, None, None);
//! assert_eq!(cat, SubMsFeatureCategory::Structural);
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// How a feature relates to the latency claim. The bench decides this from a
/// size sweep; the string form is the manifest wire value + the UI enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubMsFeatureCategory {
    /// Per-op, size-independent latency -> a measured p99 claim.
    HotPath,
    /// O(n) whole-structure op -> excluded from the per-op sub-ms claim.
    Structural,
    /// No hot-path workload, or a measured non-effect -> capability, no claim.
    Auxiliary,
}

impl SubMsFeatureCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            SubMsFeatureCategory::HotPath => "hot-path",
            SubMsFeatureCategory::Structural => "structural",
            SubMsFeatureCategory::Auxiliary => "auxiliary",
        }
    }
    /// Parse the manifest wire value (`hot-path` / `structural` / `auxiliary`).
    pub fn from_wire(s: &str) -> Option<SubMsFeatureCategory> {
        match s {
            "hot-path" => Some(SubMsFeatureCategory::HotPath),
            "structural" => Some(SubMsFeatureCategory::Structural),
            "auxiliary" => Some(SubMsFeatureCategory::Auxiliary),
            _ => None,
        }
    }
}

/// A feature that grows its p99 by at least this fraction of the relative size
/// growth is O(n)-ish -> structural. 0.5 keeps O(log n) / O(1) (a treap op, a
/// filter probe) firmly on the hot path while catching genuine linear scaling.
const STRUCTURAL_FRACTION: f64 = 0.5;
/// A flat feature whose p99 is within this fraction of the base op is a measured
/// non-effect -> auxiliary (e.g. a metrics counter that adds nothing observable).
const HOT_PATH_DELTA: f64 = 0.10;

/// Decide a feature's category from a size sweep of `(structure_size, p99_ns)`
/// measurements. `base_p99_ns` is the base op's p99 for the no-delta check
/// (None -> any flat, non-empty sweep reads as hot-path). `override_category`
/// short-circuits the decision for a genuinely ambiguous feature (amortized,
/// borderline-flat O(log n)); the reason records that it was overridden.
///
/// Returns the category and a short human reason, which the manifest stores so
/// the call is auditable.
pub fn classify_feature(
    sweep: &[(usize, u64)],
    base_p99_ns: Option<u64>,
    override_category: Option<SubMsFeatureCategory>,
) -> (SubMsFeatureCategory, String) {
    if let Some(cat) = override_category {
        return (cat, format!("override: pinned {}", cat.as_str()));
    }
    if sweep.is_empty() {
        return (
            SubMsFeatureCategory::Auxiliary,
            "no hot-path workload registered".to_string(),
        );
    }
    // Smallest + largest measured size. A single point can't show a slope, so it
    // falls through to the base-delta test (flat by assumption).
    let (min_n, p99_at_min) = sweep.iter().min_by_key(|(n, _)| *n).copied().unwrap();
    let (max_n, p99_at_max) = sweep.iter().max_by_key(|(n, _)| *n).copied().unwrap();

    if max_n > min_n && p99_at_min > 0 {
        let size_ratio = max_n as f64 / min_n as f64;
        let p99_ratio = p99_at_max as f64 / p99_at_min as f64;
        // Structural iff p99 grew by at least STRUCTURAL_FRACTION of the size
        // growth: p99_ratio - 1 >= frac * (size_ratio - 1).
        if p99_ratio - 1.0 >= STRUCTURAL_FRACTION * (size_ratio - 1.0) {
            return (
                SubMsFeatureCategory::Structural,
                format!(
                    "p99 scales with size ({:.1}x over {:.0}x N) - O(n)+, excluded from per-op claim",
                    p99_ratio, size_ratio
                ),
            );
        }
    }

    // Flat / sub-linear. Above the base op by a real margin -> hot-path;
    // indistinguishable from base -> a measured non-effect -> auxiliary.
    let feature_p99 = p99_at_max.max(p99_at_min);
    match base_p99_ns {
        Some(base) if base > 0 => {
            if feature_p99 as f64 > base as f64 * (1.0 + HOT_PATH_DELTA) {
                (
                    SubMsFeatureCategory::HotPath,
                    format!("flat per-op p99 {}ns, above base {}ns", feature_p99, base),
                )
            } else if feature_p99 < base {
                // Faster than base is still "no cost on the hot path", but it is
                // NOT "within 10% of base" - saying so of a figure a third of the
                // baseline makes the recorded reason wrong, and the reason is the
                // only audit trail the category has.
                (
                    SubMsFeatureCategory::Auxiliary,
                    format!(
                        "flat p99 {}ns, at or below base {}ns - no hot-path cost",
                        feature_p99, base
                    ),
                )
            } else {
                (
                    SubMsFeatureCategory::Auxiliary,
                    format!(
                        "flat p99 {}ns within {:.0}% of base {}ns - measured non-effect",
                        feature_p99,
                        HOT_PATH_DELTA * 100.0,
                        base
                    ),
                )
            }
        }
        _ => (
            SubMsFeatureCategory::HotPath,
            format!("flat per-op p99 {}ns", feature_p99),
        ),
    }
}

// ---------------- zero-dep JSON value model ----------------

/// A minimal JSON value. Objects keep insertion order (a `Vec` of pairs, not a
/// map) so a merge preserves the on-disk key order and any unknown fields.
/// Numbers keep their literal text so large integer p99 values round-trip exactly.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn as_object_mut(&mut self) -> Option<&mut Vec<(String, Json)>> {
        match self {
            Json::Obj(v) => Some(v),
            _ => None,
        }
    }
    fn get_mut<'a>(obj: &'a mut [(String, Json)], key: &str) -> Option<&'a mut Json> {
        obj.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    /// Read a top-level key off this value, or `None` if it is not an object.
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(v) => v.iter().find(|(k, _)| k == key).map(|(_, val)| val),
            _ => None,
        }
    }
    /// Drop `key` if present. Used to clear a stamp that no longer applies, so
    /// a local re-run cannot leave a stale fleet reference behind.
    fn remove(obj: &mut Vec<(String, Json)>, key: &str) {
        obj.retain(|(k, _)| k != key);
    }

    /// Set `key` on an object, preserving position if it already exists.
    fn set(obj: &mut Vec<(String, Json)>, key: &str, val: Json) {
        if let Some(slot) = obj.iter_mut().find(|(k, _)| k == key) {
            slot.1 = val;
        } else {
            obj.push((key.to_string(), val));
        }
    }

    /// Serialise to compact, deterministic JSON (2-space indent for readability;
    /// stable key order = insertion order).
    pub fn to_pretty(&self) -> String {
        let mut s = String::new();
        self.write_pretty(&mut s, 0);
        s.push('\n');
        s
    }

    fn write_pretty(&self, out: &mut String, indent: usize) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Num(n) => out.push_str(n),
            Json::Str(s) => write_json_string(out, s),
            Json::Arr(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push_str("[\n");
                for (i, it) in items.iter().enumerate() {
                    pad(out, indent + 1);
                    it.write_pretty(out, indent + 1);
                    if i + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, indent);
                out.push(']');
            }
            Json::Obj(pairs) => {
                if pairs.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{\n");
                for (i, (k, v)) in pairs.iter().enumerate() {
                    pad(out, indent + 1);
                    write_json_string(out, k);
                    out.push_str(": ");
                    v.write_pretty(out, indent + 1);
                    if i + 1 < pairs.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, indent);
                out.push('}');
            }
        }
    }
}

fn pad(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Parse JSON text into a [`Json`] value. Returns an error string on malformed
/// input. Std-only recursive descent; enough for manifest documents.
pub fn parse_json(input: &str) -> Result<Json, String> {
    let mut p = JsonParser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    p.skip_ws();
    let v = p.value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(format!("trailing input at byte {}", p.pos));
    }
    Ok(v)
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }
    fn value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') | Some(b'f') => self.boolean(),
            Some(b'n') => self.null(),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("unexpected byte {:?} at {}", c as char, self.pos)),
            None => Err("unexpected end of input".to_string()),
        }
    }
    fn object(&mut self) -> Result<Json, String> {
        self.pos += 1; // {
        let mut out = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Obj(out));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(format!("expected ':' at {}", self.pos));
            }
            self.pos += 1;
            let val = self.value()?;
            out.push((key, val));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(format!("expected ',' or '}}' at {}", self.pos)),
            }
        }
        Ok(Json::Obj(out))
    }
    fn array(&mut self) -> Result<Json, String> {
        self.pos += 1; // [
        let mut out = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Arr(out));
        }
        loop {
            let v = self.value()?;
            out.push(v);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(format!("expected ',' or ']' at {}", self.pos)),
            }
        }
        Ok(Json::Arr(out))
    }
    fn string(&mut self) -> Result<String, String> {
        if self.peek() != Some(b'"') {
            return Err(format!("expected string at {}", self.pos));
        }
        self.pos += 1;
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return Err("unterminated string".to_string()),
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some(b'"') => s.push('"'),
                        Some(b'\\') => s.push('\\'),
                        Some(b'/') => s.push('/'),
                        Some(b'n') => s.push('\n'),
                        Some(b't') => s.push('\t'),
                        Some(b'r') => s.push('\r'),
                        Some(b'b') => s.push('\u{0008}'),
                        Some(b'f') => s.push('\u{000C}'),
                        Some(b'u') => {
                            let hex = self
                                .bytes
                                .get(self.pos + 1..self.pos + 5)
                                .ok_or("bad \\u escape")?;
                            let code = u32::from_str_radix(
                                std::str::from_utf8(hex).map_err(|_| "bad \\u hex")?,
                                16,
                            )
                            .map_err(|_| "bad \\u hex")?;
                            s.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                            self.pos += 4;
                        }
                        _ => return Err("bad escape".to_string()),
                    }
                    self.pos += 1;
                }
                Some(_) => {
                    // Copy the whole UTF-8 char.
                    let start = self.pos;
                    let ch_len = utf8_len(self.bytes[self.pos]);
                    self.pos += ch_len;
                    let slice = &self.bytes[start..self.pos.min(self.bytes.len())];
                    s.push_str(std::str::from_utf8(slice).map_err(|_| "bad utf8")?);
                }
            }
        }
        Ok(s)
    }
    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || c == b'+' || c == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.pos]).map_err(|_| "bad number")?;
        Ok(Json::Num(raw.to_string()))
    }
    fn boolean(&mut self) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(b"true") {
            self.pos += 4;
            Ok(Json::Bool(true))
        } else if self.bytes[self.pos..].starts_with(b"false") {
            self.pos += 5;
            Ok(Json::Bool(false))
        } else {
            Err(format!("bad literal at {}", self.pos))
        }
    }
    fn null(&mut self) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(b"null") {
            self.pos += 4;
            Ok(Json::Null)
        } else {
            Err(format!("bad literal at {}", self.pos))
        }
    }
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

// ---------------- p99 provenance ----------------

/// Which box a manifest's `p99ByStage` figures were measured on.
///
/// The feature bench runs wherever it is invoked, so a manifest that carries
/// numbers without saying where they came from is indistinguishable from one
/// captured on the conformance box. Consumers treat an unstamped manifest as
/// [`SubMsP99Source::Local`] and do not publish its numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SubMsP99Source {
    /// A dev machine. The CATEGORY still holds - it is read from the shape of a
    /// size sweep, which does not depend on the box - but the numbers do not.
    Local,
    /// The conformance fleet box, identified by its EC2 instance id.
    Fleet,
}

impl SubMsP99Source {
    /// The lowercase wire token written to `.subms/features/<lang>.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            SubMsP99Source::Local => "local",
            SubMsP99Source::Fleet => "fleet",
        }
    }

    /// Parse a wire token. Anything unrecognised reads as `Local`, so a typo
    /// withholds numbers instead of publishing them.
    pub fn from_wire(s: &str) -> Self {
        match s {
            "fleet" => SubMsP99Source::Fleet,
            _ => SubMsP99Source::Local,
        }
    }

    /// Read provenance from the environment: `(Fleet, Some(id))` when
    /// `SUBMS_FLEET_INSTANCE` names a box, `(Local, None)` otherwise.
    ///
    /// The env var is the contract between the fleet orchestrator and every
    /// recipe's `perf_features` target, so no recipe hand-rolls its own
    /// detection - and a run anywhere else is Local by omission rather than by
    /// remembering to say so.
    pub fn from_env() -> (Self, Option<String>) {
        match std::env::var("SUBMS_FLEET_INSTANCE") {
            Ok(id) if !id.trim().is_empty() => (SubMsP99Source::Fleet, Some(id.trim().to_string())),
            _ => (SubMsP99Source::Local, None),
        }
    }
}

// ---------------- the feature manifest ----------------

/// A per-language feature manifest (`.subms/features/<lang>.json`). Wraps a
/// [`Json`] object so a load/mutate/save round-trip preserves every field the
/// harness does not own - only a feature's `perf` rating + `p99ByStage` are set.
pub struct SubMsFeatureManifest {
    root: Json,
}

impl SubMsFeatureManifest {
    /// An empty manifest for `lang` (`{ "lang": <lang>, "features": {} }`).
    pub fn new(lang: &str) -> Self {
        SubMsFeatureManifest {
            root: Json::Obj(vec![
                ("lang".to_string(), Json::Str(lang.to_string())),
                ("features".to_string(), Json::Obj(Vec::new())),
            ]),
        }
    }

    /// Parse an existing manifest document, PRESERVING all fields. Falls back to
    /// an empty manifest for `lang` if the text is empty or not an object.
    pub fn load_str(lang: &str, text: &str) -> Self {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Self::new(lang);
        }
        match parse_json(trimmed) {
            Ok(root @ Json::Obj(_)) => {
                let mut m = SubMsFeatureManifest { root };
                m.ensure_features_obj();
                m
            }
            _ => Self::new(lang),
        }
    }

    /// Stamp which box the `p99ByStage` figures in this manifest came from.
    ///
    /// A latency number describes the machine that produced it, and the feature
    /// bench runs wherever the author happens to be. Without this stamp a
    /// consumer cannot tell a conformance-box capture from a laptop run, and the
    /// site's renderer will not publish an unstamped number.
    ///
    /// `reference` identifies the box when the source is
    /// [`SubMsP99Source::Fleet`] - the EC2 instance id the capture ran on. It is
    /// ignored for a local run, and any stale reference is cleared, so a manifest
    /// cannot keep pointing at a fleet box after being re-run on a laptop.
    pub fn set_p99_source(&mut self, source: SubMsP99Source, reference: Option<&str>) {
        let root = match self.root.as_object_mut() {
            Some(r) => r,
            None => return,
        };
        Json::set(root, "p99_source", Json::Str(source.as_str().to_string()));
        match (source, reference) {
            (SubMsP99Source::Fleet, Some(r)) if !r.is_empty() => {
                Json::set(root, "p99_source_ref", Json::Str(r.to_string()))
            }
            _ => Json::remove(root, "p99_source_ref"),
        }
    }

    /// The provenance currently recorded. An unstamped manifest reads as
    /// [`SubMsP99Source::Local`] - the conservative direction, since it withholds
    /// numbers rather than publishing unattributed ones.
    pub fn p99_source(&self) -> SubMsP99Source {
        match self.root.get("p99_source") {
            Some(Json::Str(s)) => SubMsP99Source::from_wire(s),
            _ => SubMsP99Source::Local,
        }
    }

    /// The EC2 instance id a fleet capture was stamped with, if any.
    pub fn p99_source_ref(&self) -> Option<&str> {
        match self.root.get("p99_source_ref") {
            Some(Json::Str(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    fn ensure_features_obj(&mut self) {
        if let Json::Obj(root) = &mut self.root {
            let has = root
                .iter()
                .any(|(k, v)| k == "features" && matches!(v, Json::Obj(_)));
            if !has {
                Json::set(root, "features", Json::Obj(Vec::new()));
            }
        }
    }

    /// Set (merge) a feature's rating + p99-by-stage, leaving every other field
    /// on that feature entry, on other features, and at the top level untouched.
    /// `reason` records why the category was decided (auditable).
    pub fn set_feature(
        &mut self,
        name: &str,
        category: SubMsFeatureCategory,
        p99_by_stage: &BTreeMap<String, u64>,
        reason: &str,
    ) {
        let root = match self.root.as_object_mut() {
            Some(r) => r,
            None => return,
        };
        // Get-or-create the `features` object.
        if Json::get_mut(root, "features").is_none() {
            Json::set(root, "features", Json::Obj(Vec::new()));
        }
        let features = match Json::get_mut(root, "features").and_then(Json::as_object_mut) {
            Some(f) => f,
            None => return,
        };
        // Get-or-create the feature entry, preserving any existing custom fields.
        if Json::get_mut(features, name).is_none() {
            Json::set(features, name, Json::Obj(Vec::new()));
        }
        let entry = match Json::get_mut(features, name).and_then(Json::as_object_mut) {
            Some(e) => e,
            None => return,
        };
        Json::set(entry, "perf", Json::Str(category.as_str().to_string()));
        Json::set(entry, "perfReason", Json::Str(reason.to_string()));
        if p99_by_stage.is_empty() {
            // A structural/auxiliary feature carries no p99; drop a stale one.
            entry.retain(|(k, _)| k != "p99ByStage");
        } else {
            let stages: Vec<(String, Json)> = p99_by_stage
                .iter()
                .map(|(k, v)| (k.clone(), Json::Num(v.to_string())))
                .collect();
            Json::set(entry, "p99ByStage", Json::Obj(stages));
        }
    }

    /// The rating currently recorded for a feature, if any.
    pub fn category_of(&self, name: &str) -> Option<SubMsFeatureCategory> {
        let root = match &self.root {
            Json::Obj(r) => r,
            _ => return None,
        };
        let features = root
            .iter()
            .find(|(k, _)| k == "features")
            .and_then(|(_, v)| match v {
                Json::Obj(f) => Some(f),
                _ => None,
            })?;
        let entry = features
            .iter()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| match v {
                Json::Obj(e) => Some(e),
                _ => None,
            })?;
        entry
            .iter()
            .find(|(k, _)| k == "perf")
            .and_then(|(_, v)| match v {
                Json::Str(s) => SubMsFeatureCategory::from_wire(s),
                _ => None,
            })
    }

    /// The manifest serialised back to pretty JSON.
    pub fn to_json(&self) -> String {
        self.root.to_pretty()
    }

    /// Load from a file, preserving all fields. A missing/unreadable file yields
    /// an empty manifest for `lang` (never an error - the first write creates it).
    pub fn load(lang: &str, path: &std::path::Path) -> Self {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        Self::load_str(lang, &text)
    }

    /// Write the manifest to `path`, creating parent directories. The write is
    /// the only mutation of the on-disk file, and it always emits a full, valid
    /// document (never a partial file).
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_json())
    }
}

// Unit tests live in a colocated file (org convention: `<module>_tests.rs`
// alongside the module, included here), not the top-level `tests/` dir.
#[cfg(test)]
#[path = "feature_tests.rs"]
mod tests;
