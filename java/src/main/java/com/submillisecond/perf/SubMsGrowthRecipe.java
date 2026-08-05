package com.submillisecond.perf;

import java.util.Map;

/**
 * A storage-growth workload. The harness calls {@link #op} {@link #opsPerRound}
 * times per round, then reads the three footprint hooks.
 *
 * <p>Port of the Rust {@code SubMsGrowthRecipe} trait. The Rust version returns
 * the declared expectation as one tuple; Java splits it into
 * {@link #expectedClass} and {@link #expectedBound} because a two-element record
 * for a pair the caller always destructures buys nothing.
 *
 * <p>The footprint hooks are the recipe's own accounting, not a heap
 * measurement - a Java curve is therefore the same shape as the Rust one and is
 * not reading the GC.
 */
public interface SubMsGrowthRecipe {

    /** Workload identity, e.g. "subms-lsm-tree". */
    String name();

    /** Short op label for the page, e.g. "put". */
    default String opName() {
        return "op";
    }

    /** Number of rounds R. */
    int rounds();

    /** Timed ops performed (and measured for p50/p99) each round. */
    int opsPerRound();

    /** One timed unit of work. {@code round} is 1-based, {@code i} 0-based within it. */
    void op(int round, int i);

    /**
     * Called once after a round's timed ops, before the footprint hooks. Force a
     * flush or checkpoint here so on-disk reflects the round's writes. Not timed.
     */
    default void endRound(int round) {}

    /** Bytes physically on disk right now. 0 for a pure in-memory recipe. */
    default long diskBytes() {
        return 0L;
    }

    /**
     * Resident memory bytes the structure holds right now - the recipe's own
     * estimate (entries * entry size, an arena's used bytes). 0 when the
     * footprint is purely on disk.
     */
    default long memoryBytes() {
        return 0L;
    }

    /**
     * Logical bytes the store must retain - what a from-scratch rewrite of the
     * current key set would cost. The denominator for amplification.
     */
    long liveBytes();

    /** Named structure counts at this round, e.g. {@code {"sstables": 6}}. */
    default Map<String, Long> structures() {
        return Map.of();
    }

    /** The declared expectation. */
    SubMsGrowth.GrowthClass expectedClass();

    /**
     * Its bound: bytes for BOUNDED, a ratio for AMPLIFICATION_BOUNDED and
     * PLATEAU_BOUNDED, ignored for UNBOUNDED_OK. Applied to the TOTAL footprint.
     */
    double expectedBound();

    /**
     * Render hint: a recipe whose footprint is O(1) by construction has no
     * interesting curve, so the page shows a verdict card instead of a chart.
     */
    default boolean compact() {
        return false;
    }
}
