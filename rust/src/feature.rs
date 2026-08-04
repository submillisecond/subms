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
    /// Flat and per-op, but above the sub-ms claim line -> REPORTED, not claimed.
    ///
    /// Distinct from `Structural`, which means O(n): an op can be genuinely
    /// size-independent and still cost 30 ms. Without this the classifier had no
    /// upper bound at all and published such a figure as a per-op claim.
    Reported,
    /// The measurement cannot separate this feature from the guard that would
    /// decide it -> no category, and the reason says which test was too close.
    ///
    /// Not a failure. A feature that genuinely costs about what the base op
    /// costs has no true side of a 10% line, and picking one produces a verdict
    /// that flips between runs of unchanged code while looking authoritative.
    Indeterminate,
}

impl SubMsFeatureCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            SubMsFeatureCategory::HotPath => "hot-path",
            SubMsFeatureCategory::Structural => "structural",
            SubMsFeatureCategory::Auxiliary => "auxiliary",
            SubMsFeatureCategory::Reported => "reported",
            SubMsFeatureCategory::Indeterminate => "indeterminate",
        }
    }
    /// Parse the manifest wire value (`hot-path` / `structural` / `auxiliary` /
    /// `reported` / `indeterminate`).
    pub fn from_wire(s: &str) -> Option<SubMsFeatureCategory> {
        match s {
            "hot-path" => Some(SubMsFeatureCategory::HotPath),
            "structural" => Some(SubMsFeatureCategory::Structural),
            "auxiliary" => Some(SubMsFeatureCategory::Auxiliary),
            "reported" => Some(SubMsFeatureCategory::Reported),
            "indeterminate" => Some(SubMsFeatureCategory::Indeterminate),
            _ => None,
        }
    }
}

/// One stage of a feature, classified on its own measurements.
///
/// A feature is not necessarily one kind of operation. `concurrent-reads` on the
/// adaptive radix tree has a 1.2 us `get` and a 44 ms `snapshot`; a single
/// category cannot describe both, and the rolled-up one published the snapshot
/// under the get's label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubMsStageClass {
    pub p99_ns: u64,
    pub category: SubMsFeatureCategory,
    pub reason: String,
}

impl SubMsFeatureCategory {
    /// How much a category restricts what the feature may claim. The rollup takes
    /// the maximum, so one heavy stage cannot hide behind a fast one.
    fn restriction(self) -> u8 {
        match self {
            // No measurable cost: restricts nothing.
            SubMsFeatureCategory::Auxiliary => 0,
            // A claim, but a claim nonetheless.
            SubMsFeatureCategory::HotPath => 1,
            // Excluded from the per-op claim, for a known reason.
            SubMsFeatureCategory::Structural => 2,
            SubMsFeatureCategory::Reported => 3,
            // Not knowing is worse than knowing it is excluded.
            SubMsFeatureCategory::Indeterminate => 4,
        }
    }
}

/// Roll per-stage categories into the one a feature publishes.
///
/// The MOST RESTRICTIVE stage wins. A feature whose stages are `hot-path` and
/// `reported` is not a hot-path feature - it contains an operation that cannot
/// carry the claim, and summarising it as hot-path is how a 44 ms snapshot came
/// to sit under a per-op sub-ms label. The per-stage detail stays in the
/// manifest, so the summary being conservative costs the reader nothing.
pub fn roll_up_stages(stages: &BTreeMap<String, SubMsStageClass>) -> SubMsFeatureCategory {
    stages
        .values()
        .map(|s| s.category)
        .max_by_key(|c| c.restriction())
        .unwrap_or(SubMsFeatureCategory::Auxiliary)
}

/// A feature that grows its p99 by at least this fraction of the relative size
/// growth is O(n)-ish -> structural. 0.5 keeps O(log n) / O(1) (a treap op, a
/// filter probe) firmly on the hot path while catching genuine linear scaling.
const STRUCTURAL_FRACTION: f64 = 0.5;
/// A flat feature whose p99 is within this fraction of the base op is a measured
/// non-effect -> auxiliary (e.g. a metrics counter that adds nothing observable).
const HOT_PATH_DELTA: f64 = 0.10;
/// Per-op figures above this are REPORTED, never claimed. The site's whole
/// promise is p99 < 1 ms, so a flat op costing more than that has no business
/// carrying a per-op claim however size-independent it is.
const CLAIM_LINE_NS: u64 = 1_000_000;
/// Scaling test: how close to the structural guard counts as "too close to
/// call", as a fraction of the growth the guard demands.
///
/// Calibrated against observed instability, not taste. Two features flipped
/// between consecutive runs of UNCHANGED code on the same box: one at 1.01x of
/// its guard (a 3 ns move), one whose own p99 shifted 0.2% while its BASE moved
/// 36%. 0.25 covers both without swallowing features that are clearly one side.
///
/// It does not make every verdict stable - the 36% case can still land inside
/// the band on one run and outside on the next. That is a limit of comparing
/// against a base which is itself re-measured, and the honest read is that
/// `Indeterminate` narrows the lie rather than eliminating it.
const INDETERMINATE_BAND: f64 = 0.25;
/// Base-delta test: half-width of the undecidable band, in EXCESS over base.
/// Indeterminate when the excess lands in `HOT_PATH_DELTA +/- this` - 5%..15%
/// above base.
///
/// Deliberately expressed on the EXCESS rather than on the guard. The guard sits
/// only 10% above base, so a band around the guard wide enough to catch a
/// straddler also swallows a feature measuring exactly base - the least
/// ambiguous auxiliary there is. Banding the excess keeps "no delta" firmly
/// auxiliary while still declining on the features that actually flip.
const BASE_DELTA_BAND: f64 = 0.05;

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
        let needed = STRUCTURAL_FRACTION * (size_ratio - 1.0);
        let growth = p99_ratio - 1.0;
        if needed > 0.0 {
            let margin = growth / needed;
            if (1.0 - INDETERMINATE_BAND..=1.0 + INDETERMINATE_BAND).contains(&margin) {
                return (
                    SubMsFeatureCategory::Indeterminate,
                    format!(
                        "p99 grew {:.1}x over {:.0}x N, against {:.1}x needed for structural - too close to call",
                        p99_ratio,
                        size_ratio,
                        needed + 1.0
                    ),
                );
            }
        }
        if growth >= needed {
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
    // Above the claim line the hot-path/auxiliary question is moot: whatever its
    // delta against base, a figure this size cannot be a per-op sub-ms claim.
    // This bound is what stops a flat 30 ms op being published as one.
    if feature_p99 > CLAIM_LINE_NS {
        return (
            SubMsFeatureCategory::Reported,
            format!(
                "flat per-op p99 {}ns, above the {}ms claim line - reported, not claimed",
                feature_p99,
                CLAIM_LINE_NS / 1_000_000
            ),
        );
    }
    match base_p99_ns {
        Some(base) if base > 0 => {
            let excess = feature_p99 as f64 / base as f64 - 1.0;
            if (HOT_PATH_DELTA - BASE_DELTA_BAND..=HOT_PATH_DELTA + BASE_DELTA_BAND)
                .contains(&excess)
            {
                return (
                    SubMsFeatureCategory::Indeterminate,
                    format!(
                        "flat p99 {}ns is {:.0}% over base {}ns, against a {:.0}% hot-path guard - too close to call",
                        feature_p99,
                        excess * 100.0,
                        base,
                        HOT_PATH_DELTA * 100.0
                    ),
                );
            }
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
    /// Provenance for features written in THIS run, stamped onto each one by
    /// `set_feature`.
    ///
    /// The file-level `p99_source` is a summary and cannot be trusted alone: the
    /// manifest MERGE-writes, so a feature the current run did not measure keeps
    /// its previous numbers while inheriting whatever the file says. That is how
    /// a laptop-measured feature ended up inside a file stamped `fleet`.
    run_source: Option<(SubMsP99Source, Option<String>)>,
}

impl SubMsFeatureManifest {
    /// An empty manifest for `lang` (`{ "lang": <lang>, "features": {} }`).
    pub fn new(lang: &str) -> Self {
        SubMsFeatureManifest {
            run_source: None,
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
                let mut m = SubMsFeatureManifest {
                    root,
                    run_source: None,
                };
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
        self.run_source = Some((source, reference.map(|r| r.to_string())));
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
        // Stamp the feature with the provenance of THIS run. The file-level
        // field says where the file was last touched; this says where these
        // numbers came from, which is the only one a reader can act on when the
        // merge has left older features in place beside fresh ones.
        match &self.run_source {
            Some((source, reference)) => {
                Json::set(entry, "p99Source", Json::Str(source.as_str().to_string()));
                match (source, reference) {
                    (SubMsP99Source::Fleet, Some(r)) if !r.is_empty() => {
                        Json::set(entry, "p99SourceRef", Json::Str(r.clone()))
                    }
                    _ => Json::remove(entry, "p99SourceRef"),
                }
            }
            // Nothing declared this run: drop a stale stamp rather than let a
            // previous run's provenance describe numbers it did not produce.
            None => {
                Json::remove(entry, "p99Source");
                Json::remove(entry, "p99SourceRef");
            }
        }
    }

    /// Merge-write a feature classified PER STAGE.
    ///
    /// Writes `stages` (each with its own p99, category and reason) and sets the
    /// feature-level `perf` to the rollup, so a consumer reading only the summary
    /// still cannot mistake a mixed feature for a uniformly hot one. Preserves
    /// custom fields exactly like `set_feature`, and drops the older flat
    /// `p99ByStage` on the features it rewrites so the two shapes never disagree.
    pub fn set_feature_stages(
        &mut self,
        name: &str,
        stages: &BTreeMap<String, SubMsStageClass>,
        reason: &str,
    ) {
        let rolled = roll_up_stages(stages);
        // Reuse set_feature for the entry plumbing, provenance stamp and rollup,
        // passing the per-stage p99s so a reader of the old shape still sees them.
        let flat: BTreeMap<String, u64> =
            stages.iter().map(|(k, v)| (k.clone(), v.p99_ns)).collect();
        self.set_feature(name, rolled, &flat, reason);
        let root = match self.root.as_object_mut() {
            Some(r) => r,
            None => return,
        };
        let features = match Json::get_mut(root, "features").and_then(Json::as_object_mut) {
            Some(f) => f,
            None => return,
        };
        let entry = match Json::get_mut(features, name).and_then(Json::as_object_mut) {
            Some(e) => e,
            None => return,
        };
        let rows: Vec<(String, Json)> = stages
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    Json::Obj(vec![
                        ("p99".to_string(), Json::Num(v.p99_ns.to_string())),
                        (
                            "perf".to_string(),
                            Json::Str(v.category.as_str().to_string()),
                        ),
                        ("perfReason".to_string(), Json::Str(v.reason.clone())),
                    ]),
                )
            })
            .collect();
        if rows.is_empty() {
            Json::remove(entry, "stages");
        } else {
            Json::set(entry, "stages", Json::Obj(rows));
        }
    }

    /// The category recorded for one stage of a feature, if the manifest carries
    /// per-stage detail for it.
    pub fn stage_category(&self, feature: &str, stage: &str) -> Option<SubMsFeatureCategory> {
        let v = self
            .root
            .get("features")?
            .get(feature)?
            .get("stages")?
            .get(stage)?
            .get("perf")?;
        match v {
            Json::Str(s) => SubMsFeatureCategory::from_wire(s),
            _ => None,
        }
    }

    /// Provenance recorded for a single feature, falling back to the file-level
    /// stamp for a manifest written before per-feature provenance existed.
    ///
    /// Prefer this over the file-level field: a merge-written manifest can hold
    /// features from different runs, and only this distinguishes them.
    pub fn feature_p99_source(&self, name: &str) -> Option<SubMsP99Source> {
        fn text(v: Option<&Json>) -> Option<&str> {
            match v {
                Some(Json::Str(s)) => Some(s.as_str()),
                _ => None,
            }
        }
        let entry = self.root.get("features").and_then(|f| f.get(name));
        if let Some(s) = text(entry.and_then(|e| e.get("p99Source"))) {
            return Some(SubMsP99Source::from_wire(s));
        }
        // Pre-v2 manifests carry only the file-level stamp. Falling back to it
        // is right for a file written before per-feature provenance existed;
        // once a run has stamped the feature, that wins.
        text(self.root.get("p99_source")).map(SubMsP99Source::from_wire)
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
