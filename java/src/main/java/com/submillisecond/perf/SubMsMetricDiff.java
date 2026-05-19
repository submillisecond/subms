package com.submillisecond.perf;

/**
 * One row in a per-stage diff: a baseline value, a candidate value, and the
 * deltas between them.
 *
 * <p>{@code deltaPct} is signed: positive means the candidate is slower
 * (regression for latency metrics, improvement for throughput - the default interpretation is latency, where positive = bad).
 *
 * <p>Field shape matches Rust's {@code subms::SubMsMetricDiff}.
 */
public record SubMsMetricDiff(
        /** Metric name (e.g. {@code "p50"}, {@code "p99"}, {@code "max"}, {@code "mean"}). */ String metric,
        /** Baseline value in nanoseconds. */ long baselineNs,
        /** Candidate value in nanoseconds. */ long candidateNs,
        /** {@code candidateNs - baselineNs}. */ long deltaNs,
        /** {@code 100 * deltaNs / baselineNs}. {@code Double.POSITIVE_INFINITY} when baseline = 0. */ double deltaPct) {
}
