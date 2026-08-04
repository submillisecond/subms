package com.submillisecond.perf;

/**
 * How a library feature relates to the latency claim. The bench decides this
 * from a size sweep (see {@link SubMsFeatureManifest#classify}); the string form
 * is the manifest wire value and the website's UI enum.
 *
 * <ul>
 *   <li>{@link #HOT_PATH} - per-op, size-independent latency: a measured p99 claim.</li>
 *   <li>{@link #STRUCTURAL} - an O(n) whole-structure op (serialize, compaction);
 *       excluded from the per-op sub-ms claim.</li>
 *   <li>{@link #AUXILIARY} - no hot-path workload, or a measured non-effect: a
 *       capability with no latency claim.</li>
 * </ul>
 *
 * <p>Mirrors Rust's {@code subms::SubMsFeatureCategory}.
 */
public enum SubMsFeatureCategory {
    HOT_PATH("hot-path"),
    STRUCTURAL("structural"),
    AUXILIARY("auxiliary"),
    /**
     * Flat and per-op, but above the sub-ms claim line -&gt; REPORTED, not claimed.
     *
     * <p>Distinct from {@link #STRUCTURAL}, which means O(n): an op can be
     * genuinely size-independent and still cost 30 ms. Without this the classifier
     * had no upper bound and published such a figure as a per-op claim.
     */
    REPORTED("reported"),
    /**
     * The measurement cannot separate this feature from the guard that would
     * decide it -&gt; no category, and the reason says which test was too close.
     *
     * <p>Not a failure. A feature costing about what the base op costs has no true
     * side of a 10% line, and picking one produces a verdict that flips between
     * runs of unchanged code while looking authoritative.
     */
    INDETERMINATE("indeterminate");

    private final String wire;

    SubMsFeatureCategory(String wire) {
        this.wire = wire;
    }

    /** The manifest wire value (`hot-path` / `structural` / `auxiliary`). */
    public String asString() {
        return wire;
    }

    /** Parse the wire value; null if unrecognised. */
    public static SubMsFeatureCategory fromWire(String s) {
        if (s == null) {
            return null;
        }
        switch (s) {
            case "hot-path":
                return HOT_PATH;
            case "structural":
                return STRUCTURAL;
            case "auxiliary":
                return AUXILIARY;
            case "reported":
                return REPORTED;
            case "indeterminate":
                return INDETERMINATE;
            default:
                return null;
        }
    }
}
