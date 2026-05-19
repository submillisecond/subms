package com.submillisecond.perf;

import java.util.List;

/**
 * Per-stage diff: every percentile + mean compared between two runs of the
 * same stage. {@code worstRegressionPct} is the most-positive deltaPct across
 * all metrics - the headline number a CI gate keys off.
 *
 * <p>Field shape matches Rust's {@code subms::SubMsStageDiff}.
 */
public record SubMsStageDiff(
        /** Stage name. */ String stage,
        /** One {@link SubMsMetricDiff} per percentile + mean, in display order. */ List<SubMsMetricDiff> metrics,
        /** Largest positive deltaPct across {@link #metrics}. {@code 0.0} if all metrics improved. */ double worstRegressionPct) {
}
