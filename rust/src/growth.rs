//! Storage-growth harness. Where the latency harness ([`crate::SubMsPerfHarness`])
//! answers "how fast is one op", this answers "does the footprint stay bounded as
//! work accumulates" - the leak / write-amplification axis.
//!
//! A recipe runs R rounds of a write/fork workload. After each round the harness
//! records the physical footprint (`on_disk_bytes`), the logical live size
//! (`live_bytes`), named structure counts (SSTables, rooms, ...), and the round's
//! op p50/p99. It then gates the curve against the author's declared expectation
//! ([`SubMsGrowthClass`]): a store that grows super-linearly while live data is
//! flat (a compaction leak, or accumulating forks) FAILS its own gate.
//!
//! Rust-only for now (the storage-backed recipes measured here - lsm-tree,
//! icehouse - own their on-disk format on the Rust side).

use std::collections::BTreeMap;
// `write!` into the String buffer needs fmt::Write in scope; the JSON sink bound
// (`W: Write`) is io::Write. Import fmt's anonymously so `Write` still names io's.
use std::fmt::Write as _;
use std::io::{self, Write};
use std::time::Instant;

use crate::stats;

/// Growth JSON schema version, stamped into every emitted file. Consumers gate
/// "current model?" on this the same way latency files use `bench_version`.
pub const GROWTH_VERSION: u32 = 2;

/// What footprint growth a recipe's author expects - and the shape the harness
/// gates on. The paired `bound` (see [`SubMsGrowthRecipe::expected`]) is
/// interpreted per variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubMsGrowthClass {
    /// Footprint is capped regardless of ops (e.g. a fixed-capacity cache).
    /// Gate: peak `on_disk_bytes` across rounds <= `bound` bytes.
    Bounded,
    /// On-disk stays proportional to live data - amplification is bounded by a
    /// constant. Gate: max(`on_disk_bytes` / `live_bytes`) <= `bound`. This is
    /// the write-amplification / accumulating-garbage catch: if live is flat but
    /// on-disk climbs every round, the ratio blows past the ceiling.
    AmplificationBounded,
    /// On-disk PLATEAUS - it may pre-allocate to a steady size, but must not keep
    /// climbing round over round. Gate: `on_disk`(last round) <= `on_disk`(first
    /// round) * `bound`. This is the reclaim / leak catch for churn workloads
    /// (calve+drop, insert+delete) where live data is flat and the absolute size
    /// is dominated by allocator behaviour, not the payload - so an amplification
    /// ratio against a tiny live set would false-alarm, but sustained growth is
    /// still a real leak.
    PlateauBounded,
    /// Growth is expected and unbounded by design (an append-only log). Reported,
    /// never gated.
    UnboundedOk,
}

impl SubMsGrowthClass {
    /// The stable snake_case token used in the JSON `verdict.class` field.
    pub fn as_str(self) -> &'static str {
        match self {
            SubMsGrowthClass::Bounded => "bounded",
            SubMsGrowthClass::AmplificationBounded => "amplification_bounded",
            SubMsGrowthClass::PlateauBounded => "plateau_bounded",
            SubMsGrowthClass::UnboundedOk => "unbounded_ok",
        }
    }
}

/// A storage-growth workload. The harness calls `op` `ops_per_round` times per
/// round, then reads the three footprint hooks. All `&mut self` so a hook may
/// stat files or walk the store; keep them cheap (called once per round).
pub trait SubMsGrowthRecipe {
    /// Workload identity, e.g. "subms-lsm-tree" or "icehouse-catalog".
    fn name(&self) -> &str;
    /// Short op label for the page, e.g. "put" or "fork".
    fn op_name(&self) -> &str {
        "op"
    }
    /// Number of rounds R.
    fn rounds(&self) -> usize;
    /// Timed ops performed (and measured for p50/p99) each round.
    fn ops_per_round(&self) -> usize;
    /// One timed unit of work. `round` is 1-based; `i` is 0-based within the round.
    fn op(&mut self, round: usize, i: usize);
    /// Called once after a round's timed ops, before the footprint hooks. Use it
    /// to force a flush/checkpoint so on-disk reflects the round's writes. The
    /// work here is NOT timed. Default: no-op.
    fn end_round(&mut self, _round: usize) {}
    /// Bytes physically ON DISK right now (sum of the store's file sizes). 0 for a
    /// pure in-memory recipe. Together with [`Self::memory_bytes`] this is the
    /// footprint the verdict gates on (their sum).
    fn disk_bytes(&mut self) -> u64 {
        0
    }
    /// Resident MEMORY bytes the structure holds right now - the recipe's best
    /// estimate of its heap footprint (e.g. entries * entry_size, or an arena's
    /// used bytes). 0 for a recipe whose footprint is purely on disk.
    fn memory_bytes(&mut self) -> u64 {
        0
    }
    /// Logical/live bytes the store must retain - what a from-scratch rewrite of
    /// the current key set would cost. The denominator for amplification.
    fn live_bytes(&mut self) -> u64;
    /// Named structure counts at this round, e.g. `[("sstables", 6)]`.
    fn structures(&mut self) -> Vec<(String, u64)> {
        Vec::new()
    }
    /// The declared expectation and its bound (bytes for `Bounded`, a ratio for
    /// `AmplificationBounded`, ignored for `UnboundedOk`). The gate is applied to
    /// the TOTAL footprint (disk + memory).
    fn expected(&self) -> (SubMsGrowthClass, f64);
    /// Render hint: a recipe whose footprint is O(1) by construction (a single
    /// bucket, fixed histogram) has no interesting curve - set this so the page
    /// shows a compact verdict card instead of a chart + heatmap.
    fn compact(&self) -> bool {
        false
    }
}

/// One round's captured footprint + op latency.
#[derive(Debug, Clone)]
pub struct SubMsGrowthRound {
    pub round: usize,
    pub ops: usize,
    pub cumulative_ops: usize,
    /// Bytes on disk (0 for in-memory recipes).
    pub disk_bytes: u64,
    /// Resident memory bytes (0 for recipes with no in-memory footprint).
    pub memory_bytes: u64,
    /// disk + memory - the footprint the verdict gates on.
    pub total_bytes: u64,
    pub live_bytes: u64,
    /// total / live at this round (0.0 when live is 0).
    pub amplification: f64,
    pub structures: BTreeMap<String, u64>,
    pub p50_ns: u64,
    pub p99_ns: u64,
    pub max_ns: u64,
}

/// The gate outcome over the whole curve.
#[derive(Debug, Clone)]
pub struct SubMsGrowthVerdict {
    pub class: SubMsGrowthClass,
    pub bound: f64,
    /// Whether the observed curve satisfies the class's gate.
    pub holds: bool,
    /// The observed value the gate compares (peak bytes, or max amplification).
    pub observed: f64,
    pub summary: String,
}

/// A completed growth capture: the per-round curve + the verdict.
#[derive(Debug, Clone)]
pub struct SubMsGrowthReport {
    pub workload: String,
    pub lang: String,
    pub op_name: String,
    pub rounds: Vec<SubMsGrowthRound>,
    pub verdict: SubMsGrowthVerdict,
    /// Render hint: show a compact verdict card instead of the full chart.
    pub compact: bool,
    pub meta: BTreeMap<String, String>,
}

/// Run a growth recipe end to end: R rounds of timed ops, footprint after each,
/// then the verdict.
pub fn grow(recipe: &mut dyn SubMsGrowthRecipe, lang: &str) -> SubMsGrowthReport {
    let r = recipe.rounds().max(1);
    let ops = recipe.ops_per_round().max(1);
    let mut rounds = Vec::with_capacity(r);
    let mut cumulative = 0usize;

    for round in 1..=r {
        let mut samples = Vec::with_capacity(ops);
        for i in 0..ops {
            let t = Instant::now();
            recipe.op(round, i);
            samples.push(t.elapsed().as_nanos() as u64);
        }
        recipe.end_round(round);
        cumulative += ops;
        samples.sort_unstable();
        let disk = recipe.disk_bytes();
        let memory = recipe.memory_bytes();
        let total = disk + memory;
        let live = recipe.live_bytes();
        let amplification = if live > 0 {
            total as f64 / live as f64
        } else {
            0.0
        };
        let structures: BTreeMap<String, u64> = recipe.structures().into_iter().collect();
        rounds.push(SubMsGrowthRound {
            round,
            ops,
            cumulative_ops: cumulative,
            disk_bytes: disk,
            memory_bytes: memory,
            total_bytes: total,
            live_bytes: live,
            amplification,
            structures,
            p50_ns: stats::percentile(&samples, 0.50),
            p99_ns: stats::percentile(&samples, 0.99),
            max_ns: samples.last().copied().unwrap_or(0),
        });
    }

    let verdict = compute_verdict(recipe.expected(), &rounds);
    SubMsGrowthReport {
        workload: recipe.name().to_string(),
        lang: lang.to_string(),
        op_name: recipe.op_name().to_string(),
        rounds,
        verdict,
        compact: recipe.compact(),
        meta: BTreeMap::new(),
    }
}

fn compute_verdict(
    (class, bound): (SubMsGrowthClass, f64),
    rounds: &[SubMsGrowthRound],
) -> SubMsGrowthVerdict {
    match class {
        SubMsGrowthClass::Bounded => {
            let peak = rounds.iter().map(|r| r.total_bytes).max().unwrap_or(0) as f64;
            SubMsGrowthVerdict {
                class,
                bound,
                holds: peak <= bound,
                observed: peak,
                summary: format!(
                    "peak footprint {} bytes vs bound {} bytes",
                    peak as u64, bound as u64
                ),
            }
        }
        SubMsGrowthClass::AmplificationBounded => {
            let amp = rounds
                .iter()
                .filter(|r| r.live_bytes > 0)
                .map(|r| r.amplification)
                .fold(0.0_f64, f64::max);
            SubMsGrowthVerdict {
                class,
                bound,
                holds: amp <= bound,
                observed: amp,
                summary: format!(
                    "max footprint/live amplification {amp:.2}x vs ceiling {bound:.2}x"
                ),
            }
        }
        SubMsGrowthClass::PlateauBounded => {
            // Baseline at the mid-point, not round 1: a store with a warm-up ramp
            // (a retention window filling, an allocator pre-sizing) is small early
            // and that is not growth. What must stay flat is the second half - if
            // the footprint is still climbing from mid-run to the end, it leaks.
            let n = rounds.len();
            let mid = rounds.get(n / 2).map(|r| r.total_bytes).unwrap_or(0).max(1) as f64;
            let last = rounds.last().map(|r| r.total_bytes).unwrap_or(0) as f64;
            let ratio = last / mid;
            SubMsGrowthVerdict {
                class,
                bound,
                holds: ratio <= bound,
                observed: ratio,
                summary: format!(
                    "footprint grew {ratio:.2}x from mid-run to round {n} (ceiling {bound:.2}x)"
                ),
            }
        }
        SubMsGrowthClass::UnboundedOk => SubMsGrowthVerdict {
            class,
            bound,
            holds: true,
            observed: 0.0,
            summary: "growth expected and unbounded by design".to_string(),
        },
    }
}

/// CI gate: `Err(summary)` if the observed curve breaches its declared class.
pub fn assert_growth_holds(report: &SubMsGrowthReport) -> Result<(), String> {
    if report.verdict.holds {
        Ok(())
    } else {
        Err(format!(
            "{} growth gate breached: {}",
            report.workload, report.verdict.summary
        ))
    }
}

// ---- JSON ----

/// Emit the stable growth JSON the products/recipe page renders. Shape:
/// `{ kind:"growth", workload, lang, op, growth_version, verdict{...},
///    rounds:[{round, ops, cumulative_ops, on_disk_bytes, live_bytes,
///    amplification, structures{...}, p50_ns, p99_ns, max_ns}], meta{...} }`.
pub fn growth_to_json<W: Write>(report: &SubMsGrowthReport, out: &mut W) -> io::Result<()> {
    let mut s = String::with_capacity(256 + report.rounds.len() * 128);
    s.push('{');
    s.push_str("\"kind\":\"growth\",");
    kv_str(&mut s, "workload", &report.workload);
    s.push(',');
    kv_str(&mut s, "lang", &report.lang);
    s.push(',');
    kv_str(&mut s, "op", &report.op_name);
    s.push(',');
    let _ = write!(s, "\"growth_version\":{GROWTH_VERSION},");

    // verdict
    s.push_str("\"verdict\":{");
    kv_str(&mut s, "class", report.verdict.class.as_str());
    s.push(',');
    let _ = write!(s, "\"bound\":{:.4},", report.verdict.bound);
    let _ = write!(s, "\"holds\":{},", report.verdict.holds);
    let _ = write!(s, "\"observed\":{:.4},", report.verdict.observed);
    kv_str(&mut s, "summary", &report.verdict.summary);
    s.push_str("},");

    let _ = write!(s, "\"compact\":{},", report.compact);

    // rounds
    s.push_str("\"rounds\":[");
    for (i, r) in report.rounds.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('{');
        let _ = write!(s, "\"round\":{},", r.round);
        let _ = write!(s, "\"ops\":{},", r.ops);
        let _ = write!(s, "\"cumulative_ops\":{},", r.cumulative_ops);
        let _ = write!(s, "\"disk_bytes\":{},", r.disk_bytes);
        let _ = write!(s, "\"memory_bytes\":{},", r.memory_bytes);
        let _ = write!(s, "\"total_bytes\":{},", r.total_bytes);
        let _ = write!(s, "\"live_bytes\":{},", r.live_bytes);
        let _ = write!(s, "\"amplification\":{:.4},", r.amplification);
        s.push_str("\"structures\":{");
        for (j, (name, count)) in r.structures.iter().enumerate() {
            if j > 0 {
                s.push(',');
            }
            json_str(&mut s, name);
            let _ = write!(s, ":{count}");
        }
        s.push_str("},");
        let _ = write!(s, "\"p50_ns\":{},", r.p50_ns);
        let _ = write!(s, "\"p99_ns\":{},", r.p99_ns);
        let _ = write!(s, "\"max_ns\":{}", r.max_ns);
        s.push('}');
    }
    s.push(']');

    // meta (optional, omitted when empty)
    if !report.meta.is_empty() {
        s.push_str(",\"meta\":{");
        for (i, (k, v)) in report.meta.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            json_str(&mut s, k);
            s.push(':');
            json_str(&mut s, v);
        }
        s.push('}');
    }

    s.push('}');
    out.write_all(s.as_bytes())
}

fn kv_str(out: &mut String, k: &str, v: &str) {
    json_str(out, k);
    out.push(':');
    json_str(out, v);
}

#[cfg(test)]
#[path = "growth_tests.rs"]
mod tests;

// Minimal JSON string escaper, mirroring bench::json_str (kept local so this
// module does not widen that one's visibility).
fn json_str(out: &mut String, s: &str) {
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
