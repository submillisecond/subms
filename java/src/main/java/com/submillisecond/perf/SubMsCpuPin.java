package com.submillisecond.perf;

/** How the bench harness should place a recipe's process on the box's CPUs. */
public enum SubMsCpuPin {
    /**
     * Pin to ONE isolated core - a stable single-threaded p99. The default for a
     * single-threaded recipe; the box's {@code isolcpus} reserves the core.
     */
    SINGLE("single"),
    /**
     * Pin to {@code cores} cores. For a MULTI-threaded recipe (its own writer plus
     * a worker) that wants dedicated cores rather than starving on one.
     */
    MULTI("multi"),
    /**
     * No pinning - run across all cores on the general scheduler. For a
     * multi-threaded recipe where per-core isolation is not available/needed.
     */
    NONE("none");

    private final String wire;

    SubMsCpuPin(String wire) {
        this.wire = wire;
    }

    /** The lowercase wire token written to {@code .subms/perf/controls.json}. */
    public String wire() {
        return wire;
    }

    /**
     * Parse a wire token. Also tolerates the legacy boolean form ({@code true} ->
     * {@link #SINGLE}, {@code false} -> {@link #NONE}). Returns {@code null} on an
     * unrecognised token so the caller can apply its own default.
     */
    public static SubMsCpuPin fromWire(String s) {
        if (s == null) {
            return null;
        }
        switch (s.trim().toLowerCase()) {
            case "single":
            case "one":
            case "true":
                return SINGLE;
            case "multi":
            case "cores":
                return MULTI;
            case "none":
            case "off":
            case "false":
                return NONE;
            default:
                return null;
        }
    }
}
