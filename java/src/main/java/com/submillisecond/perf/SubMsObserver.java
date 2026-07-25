package com.submillisecond.perf;

/**
 * Observer hook: a no-op-by-default interface other code can register
 * against a {@link SubMsPerfHarness} to receive samples and summaries as
 * they happen.
 *
 * <p>The harness stays zero-dep. The hook fires only when an observer is
 * set, and costs one null-check + one interface call per recorded sample.
 *
 * <p>Sibling libraries like {@code subms-otel} provide concrete
 * implementations that bridge to OpenTelemetry / Prometheus / etc. The
 * harness itself never knows about any of them.
 *
 * <p>Mirrors Rust's {@code subms::SubMsObserver}.
 */
public interface SubMsObserver {

    /**
     * Called for each recorded sample. Fires from
     * {@link SubMsPerfHarness.Stage#record(long)},
     * {@link SubMsPerfHarness.Stage#time(Runnable)},
     * {@link SubMsPerfHarness.Stage#warmThenTime},
     * and {@link SubMsPerfHarness.PacedStage#time(Runnable)} - anywhere a
     * {@code ns} value lands in a stage's sample buffer.
     *
     * <p>Default no-op so adding methods to this interface later is non-
     * breaking.
     */
    default void onRecord(SubMsObservationCtx ctx, long ns) {}

    /**
     * Called once when {@link SubMsBench#summarize(SubMsPerfHarness)} is
     * invoked on the harness. Receives the typed summary including
     * {@code inputs} + {@code meta} + per-stage stats.
     *
     * <p>Default no-op. Other {@code summarize*} variants (lean / skipping
     * / windowed) do NOT fire the observer - they are considered internal
     * re-summarisations rather than the canonical post-bench result.
     */
    default void onSummarize(SubMsBenchSummary summary) {}
}
