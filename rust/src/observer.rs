//! Observer hook: a no-op-by-default trait other code can register against a
//! [`crate::SubMsPerfHarness`] to receive samples and summaries as they happen.
//!
//! The harness stays zero-dep. The hook fires only when an observer is set,
//! and costs one branch + one virtual call per recorded sample (~1-2 ns).
//!
//! Sibling crates like `subms-otel` provide concrete observers that bridge
//! to OpenTelemetry / Prometheus / etc. The harness itself never knows about
//! any of them.

use crate::SubMsBenchSummary;

/// What kind of operation a stage records. Observers use this to choose
/// histogram bucket boundaries that fit the measurement scale.
///
/// - `HotPath`: per-request operations under a sub-ms p99 budget
///   (`put`, `get_hit`, `enqueue`, ...).
/// - `BatchOp`: whole-structure / O(n) operations that run rarely - serialize,
///   snapshot, replay, compact, full merge. Cost scales with size.
/// - `OneShot`: setup / teardown timings recorded once or a handful of times.
///   Treated like `BatchOp` but with a lower-resolution histogram cap to
///   avoid an occasional huge value blowing the bucket count.
/// - `Unspecified`: the default when a stage doesn't declare its kind.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum SubMsStageKind {
    HotPath,
    BatchOp,
    OneShot,
    #[default]
    Unspecified,
}

impl SubMsStageKind {
    /// Lowercase snake_case identifier used as the `subms.stage.kind`
    /// OpenTelemetry attribute by sibling adapters. Kept on this side so the
    /// string is stable across crates without having to depend on `subms-otel`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HotPath => "hot_path",
            Self::BatchOp => "batch_op",
            Self::OneShot => "one_shot",
            Self::Unspecified => "unspecified",
        }
    }
}

/// Context passed to [`SubMsObserver::on_record`] for each recorded sample.
///
/// Holds borrowed references to the harness-level identity (workload, lang)
/// and the stage's identity (name, kind). Inputs and meta arrive via
/// [`SubMsObserver::on_summarize`] instead of per-record to keep this struct
/// pointer-sized (no map borrows on the hot path).
pub struct ObservationCtx<'a> {
    pub workload: &'a str,
    pub lang: &'a str,
    pub stage: &'a str,
    pub stage_kind: SubMsStageKind,
}

/// Hook a sibling crate (or downstream consumer) can register against a
/// [`crate::SubMsPerfHarness`] to react to every recorded sample plus the
/// post-bench summary. Both methods default to a no-op so adding methods to
/// the trait later is non-breaking.
///
/// `Send + Sync` are required so the same observer can be shared across
/// harness instances (e.g., one OTEL exporter wired up at process start).
pub trait SubMsObserver: Send + Sync {
    /// Called for each recorded sample. Fires from `Stage::record`,
    /// `Stage::time`, `Stage::warm_then_time`, and `PacedStage::time` -
    /// anywhere a `ns` value lands in a stage's sample buffer.
    fn on_record(&self, _ctx: &ObservationCtx, _ns: u64) {}

    /// Called once when [`crate::summarize`] is invoked on the harness.
    /// Receives the typed summary including inputs + meta + per-stage stats.
    fn on_summarize(&self, _summary: &SubMsBenchSummary) {}
}
