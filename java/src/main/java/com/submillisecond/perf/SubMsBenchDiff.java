package com.submillisecond.perf;

import java.util.List;
import java.util.Optional;

/**
 * Typed diff between a baseline and a candidate {@link SubMsBenchSummary}.
 *
 * <p>Each stage that appears in both runs becomes a {@link SubMsStageDiff}
 * with per-metric deltas. Stages that appear in only one side are listed
 * separately - those are usually structural changes (renamed stage, dropped
 * benchmark) that the CI gate should surface but not fail on.
 *
 * <p>{@code regressionThresholdPct} echoes the threshold passed to the diff
 * call, so downstream tools (CI status checks, posted comments) can render
 * the line "fail if any stage regressed > X%" without rederiving it.
 *
 * <p>Field shape matches Rust's {@code subms::SubMsBenchDiff}.
 */
public record SubMsBenchDiff(
        String baselineWorkload,
        String candidateWorkload,
        String lang,
        /** Per-stage diffs in candidate's registration order. */ List<SubMsStageDiff> stages,
        /** Stages present only in the baseline (likely renamed or removed). */ List<String> baselineOnlyStages,
        /** Stages present only in the candidate (newly added). */ List<String> candidateOnlyStages,
        /** Threshold used when computing {@link #hasRegression}. */ double regressionThresholdPct) {

    /** {@code true} if any stage's {@code worstRegressionPct} exceeded
     *  {@link #regressionThresholdPct}. */
    public boolean hasRegression() {
        for (SubMsStageDiff s : stages) {
            if (s.worstRegressionPct() > regressionThresholdPct) return true;
        }
        return false;
    }

    /** The worst-regressing stage, or empty if all stages stayed within the threshold. */
    public Optional<SubMsStageDiff> worstStage() {
        SubMsStageDiff worst = null;
        for (SubMsStageDiff s : stages) {
            if (worst == null || s.worstRegressionPct() > worst.worstRegressionPct()) worst = s;
        }
        return Optional.ofNullable(worst);
    }
}
