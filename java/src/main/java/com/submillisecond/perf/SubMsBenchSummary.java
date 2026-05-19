package com.submillisecond.perf;

import java.util.List;
import java.util.Map;
import java.util.Optional;

/**
 * Typed summary of one bench run. Mirrors the on-disk JSON shape downstream tooling persists per workload; field order
 * and naming match the Rust {@code subms::SubMsBenchSummary} struct.
 *
 * <p>Build one from a harness with {@link SubMsBench#summarize} (carries the
 * downsampled per-stage samples) or {@link SubMsBench#summarizeLean} (no
 * samples; just count + percentiles + mean).
 *
 * <p>Stage order is registration order, matching the harness.
 */
public record SubMsBenchSummary(
        String workload,
        String lang,
        String timestamp,
        Map<String, String> inputs,
        Map<String, String> meta,
        List<SubMsStageSummary> stages) {

    /** Look up a stage by name. Returns empty if no such stage was recorded. */
    public Optional<SubMsStageSummary> stage(String stageName) {
        for (SubMsStageSummary s : stages) {
            if (s.name().equals(stageName)) return Optional.of(s);
        }
        return Optional.empty();
    }
}
