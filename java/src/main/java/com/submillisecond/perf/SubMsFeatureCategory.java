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
    AUXILIARY("auxiliary");

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
            default:
                return null;
        }
    }
}
