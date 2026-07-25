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
 * <p>{@code stddevNs} is the sample standard deviation (n-1 denominator) over
 * the post-warmup-skip timings; {@code 0} when count &lt; 2. Added in
 * {@code subms} 0.4.0; older readers ignore the JSON field.
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
        long stddevNs,
        Optional<long[]> samplesNs) {
}
