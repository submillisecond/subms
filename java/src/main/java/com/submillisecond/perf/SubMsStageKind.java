package com.submillisecond.perf;

/**
 * What kind of operation a stage records. Observers (e.g. the `subms-otel`
 * sibling) use this to pick histogram bucket boundaries that fit the
 * measurement scale.
 *
 * <ul>
 *   <li>{@link #HOT_PATH} - per-request operations under a sub-ms p99 budget
 *       ({@code put}, {@code get_hit}, {@code enqueue}, ...).</li>
 *   <li>{@link #BATCH_OP} - whole-structure / O(n) operations that run rarely
 *       - serialize, snapshot, replay, compact, full merge. Cost scales with
 *       size.</li>
 *   <li>{@link #ONE_SHOT} - setup / teardown timings recorded once or a
 *       handful of times. Treated like {@code BATCH_OP} but with a lower-
 *       resolution histogram cap so an occasional huge value does not blow
 *       the bucket count.</li>
 *   <li>{@link #UNSPECIFIED} - the default when a stage does not declare its
 *       kind. Observers fall back to an exponential / default histogram.</li>
 * </ul>
 *
 * <p>Mirrors Rust's {@code subms::SubMsStageKind}. The string form returned
 * by {@link #asString()} is the stable identifier used as the
 * {@code subms.stage.kind} OpenTelemetry attribute by sibling adapters.
 */
public enum SubMsStageKind {
    HOT_PATH("hot_path"),
    BATCH_OP("batch_op"),
    ONE_SHOT("one_shot"),
    UNSPECIFIED("unspecified");

    private final String asString;

    SubMsStageKind(String asString) {
        this.asString = asString;
    }

    /** Lowercase snake_case identifier used as the
     *  {@code subms.stage.kind} OTEL attribute. Stable across versions. */
    public String asString() {
        return asString;
    }
}
