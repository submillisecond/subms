package com.submillisecond.perf;

import java.util.List;
import java.util.Optional;

/**
 * A sweep is a collection of {@link SubMsBenchSummary} runs that share a
 * workload but vary one or more input parameters - typically {@code entries}
 * for scale-curve studies, or {@code bloom_mode} for a feature-toggle
 * comparison.
 *
 * <p>Build a sweep with {@link SubMsBench#summarizeSweep} (from existing
 * summaries) or {@link SubMsBench#runSweep} (which runs the recipe N times
 * with the supplied params and summarises each run). Use
 * {@link SubMsBench#printSweep} to render a pivoted scale table or
 * {@link SubMsBench#sweepToJson} to emit the canonical multi-run
 * JSON shape (array of summaries).
 *
 * <p>{@code variedInputKey} is the input key whose value differs across runs.
 * When set (e.g. {@code "entries"} or {@code "bloom_mode"}) the print/render
 * code uses each run's value of that key as the row label. When absent, runs
 * are labelled by ordinal ({@code "run 1"}, {@code "run 2"}, ...).
 *
 * <p>Rust counterpart: {@code subms::SubMsBenchSweep}.
 */
public record SubMsBenchSweep(
        String workload,
        String lang,
        /** Input key that varied across runs, or empty if none / multiple. */ Optional<String> variedInputKey,
        List<SubMsBenchSummary> runs) {
}
