package com.submillisecond.perf;

/**
 * Context passed to {@link SubMsObserver#onRecord(SubMsObservationCtx, long)}
 * for each recorded sample. Holds the harness-level identity ({@code workload},
 * {@code lang}) and the stage's identity ({@code stage}, {@code stageKind}).
 *
 * <p>Inputs and meta are intentionally NOT on this record - they arrive via
 * {@link SubMsObserver#onSummarize(SubMsBenchSummary)} with the full summary
 * instead. That keeps this object tiny so the per-record cost is dominated by
 * the observer's emit work, not by building the context.
 *
 * <p>Mirrors Rust's {@code subms::ObservationCtx}.
 */
public record SubMsObservationCtx(
        String workload,
        String lang,
        String stage,
        SubMsStageKind stageKind) {
}
