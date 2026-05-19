package com.submillisecond.perf;

import java.util.Optional;

/**
 * Structured per-stage summary. The counterpart of one {@code "stages.<name>"}
 * entry in the standard subms JSON. Pair-of-record with {@link SubMsBenchSummary}.
 *
 * <p>{@code samplesNs} carries the same 500-sample downsampled timeline the
 * harness ships in JSON; populated by {@link SubMsBench#summarize} and absent
 * after {@link SubMsBench#summarizeLean}.
 *
 * <p>Field shape matches the Rust {@code subms::SubMsStageSummary} struct.
 */
public record SubMsStageSummary(
        String name,
        int count,
        long p50Ns,
        long p99Ns,
        long p999Ns,
        long maxNs,
        long meanNs,
        Optional<long[]> samplesNs) {
}
