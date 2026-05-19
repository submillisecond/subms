package com.submillisecond.perf;

import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SubMsBenchTest {

    @Test
    void percentileEmptyIsZero() {
        assertEquals(0L, SubMsBench.percentile(new long[0], 0.5));
    }

    @Test
    void percentileSingleValue() {
        long[] one = {42};
        assertEquals(42L, SubMsBench.percentile(one, 0.0));
        assertEquals(42L, SubMsBench.percentile(one, 0.5));
        assertEquals(42L, SubMsBench.percentile(one, 1.0));
    }

    @Test
    void percentileKnownDistribution() {
        long[] v = new long[100];
        for (int i = 0; i < 100; i++) v[i] = i + 1L;
        assertEquals(51L, SubMsBench.percentile(v, 0.50));
        assertEquals(100L, SubMsBench.percentile(v, 0.99));
        assertEquals(100L, SubMsBench.percentile(v, 0.999));
        assertEquals(100L, SubMsBench.percentile(v, 1.0));
    }

    /** Fixed-sample recipe: records 100/200/300/400 ns under stage "step". */
    static final class FixedRecipe implements SubMsRecipe {
        @Override public String name() { return "fixed-recipe"; }
        @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
            SubMsPerfHarness.Stage s = h.stage("step", 4);
            s.record(100);
            s.record(200);
            s.record(300);
            s.record(400);
        }
    }

    @Test
    void runBenchDrivesRecipe() {
        SubMsPerfHarness h = SubMsBench.runBench(new FixedRecipe(), SubMsBenchParams.defaults());
        assertNotNull(h.stage("step"));
        assertEquals(4, h.stage("step").count());
    }

    @Test
    void runBenchRecordsParamsAsInputs() throws Exception {
        SubMsPerfHarness h = SubMsBench.runBench(new FixedRecipe(), new SubMsBenchParams(123, 45, 9L));
        // serialise + spot-check the inputs panel
        java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
        try (java.io.PrintStream ps = new java.io.PrintStream(out, true, "UTF-8")) {
            h.writeJson(ps);
        }
        String json = out.toString("UTF-8");
        assertTrue(json.contains("\"entries\":\"123\""), json);
        assertTrue(json.contains("\"warmup\":\"45\""), json);
        assertTrue(json.contains("\"seed\":\"9\""), json);
    }

    @Test
    void assertP99PassesWhenAtLimit() {
        SubMsPerfHarness h = SubMsBench.runBench(new FixedRecipe(), SubMsBenchParams.defaults());
        // p99 of sorted [100,200,300,400] with idx min(3, floor(0.99*4))=3 -> 400
        SubMsBench.assertP99Under(h, List.of(new SubMsBench.Assertion("step", 400L)));
    }

    @Test
    void assertP99FailsWhenAboveLimit() {
        SubMsPerfHarness h = SubMsBench.runBench(new FixedRecipe(), SubMsBenchParams.defaults());
        AssertionError err = assertThrows(AssertionError.class, () ->
                SubMsBench.assertP99Under(h, List.of(new SubMsBench.Assertion("step", 399L))));
        String msg = err.getMessage();
        assertTrue(msg.contains("step"), msg);
        assertTrue(msg.contains("400"), msg);
        assertTrue(msg.contains("399"), msg);
    }

    @Test
    void assertP99FailsWhenStageMissing() {
        SubMsPerfHarness h = SubMsBench.runBench(new FixedRecipe(), SubMsBenchParams.defaults());
        AssertionError err = assertThrows(AssertionError.class, () ->
                SubMsBench.assertP99Under(h, List.of(new SubMsBench.Assertion("ghost", 1L))));
        assertTrue(err.getMessage().contains("ghost"));
        assertTrue(err.getMessage().contains("not found"));
    }

    @Test
    void runSweepRunsRecipeOncePerParamsSet() {
        SubMsBenchSweep sweep = SubMsBench.runSweep(
                new FixedRecipe(),
                List.of(
                        new SubMsBenchParams(4, 0, 0L),
                        new SubMsBenchParams(4, 0, 1L)),
                "seed");
        assertEquals(2, sweep.runs().size());
        assertEquals(java.util.Optional.of("seed"), sweep.variedInputKey());
        assertEquals("step", sweep.runs().get(0).stages().get(0).name());
    }

    @Test
    void summarizeSweepBundlesExistingSummaries() {
        SubMsBenchSummary a = SubMsBench.summarize(SubMsBench.runBench(new FixedRecipe(), SubMsBenchParams.defaults()));
        SubMsBenchSummary b = SubMsBench.summarize(SubMsBench.runBench(new FixedRecipe(), SubMsBenchParams.defaults()));
        SubMsBenchSweep sweep = SubMsBench.summarizeSweep(List.of(a, b), "entries");
        assertEquals(2, sweep.runs().size());
        assertEquals("fixed-recipe", sweep.workload());
        assertEquals("java", sweep.lang());
    }

    @Test
    void printSweepPivotsByStageAndLabelsRows() {
        SubMsBenchSweep sweep = SubMsBench.runSweep(
                new FixedRecipe(),
                List.of(new SubMsBenchParams(4, 0, 0L), new SubMsBenchParams(4, 0, 0L)),
                null);
        java.io.ByteArrayOutputStream buf = new java.io.ByteArrayOutputStream();
        SubMsBench.printSweep(sweep, new java.io.PrintStream(buf));
        String out = buf.toString(java.nio.charset.StandardCharsets.UTF_8);
        assertTrue(out.contains("stage: step"), out);
        assertTrue(out.contains("run 1"), out);
        assertTrue(out.contains("run 2"), out);
    }

    @Test
    void sweepToJsonEmitsArray() throws Exception {
        SubMsBenchSweep sweep = SubMsBench.runSweep(
                new FixedRecipe(),
                List.of(new SubMsBenchParams(4, 0, 0L), new SubMsBenchParams(4, 0, 0L)),
                "seed");
        java.io.ByteArrayOutputStream buf = new java.io.ByteArrayOutputStream();
        SubMsBench.sweepToJson(sweep, new java.io.PrintStream(buf));
        String out = buf.toString(java.nio.charset.StandardCharsets.UTF_8);
        assertTrue(out.startsWith("["), out);
        assertTrue(out.contains("\"workload\":\"fixed-recipe\""), out);
        assertTrue(out.contains("\"stages\":{"), out);
    }

    // ------------------------------------------------------------------
    // Diff
    // ------------------------------------------------------------------

    /** Records explicit values into one stage so the diff has predictable inputs. */
    static final class ExplicitRecipe implements SubMsRecipe {
        private final long[] values;
        ExplicitRecipe(long... values) { this.values = values; }
        @Override public String name() { return "explicit"; }
        @Override public void run(SubMsPerfHarness h, SubMsBenchParams p) {
            SubMsPerfHarness.Stage s = h.stage("put", values.length);
            for (long v : values) s.record(v);
        }
    }

    @Test
    void diffSummaryComputesPerMetricDeltas() {
        SubMsBenchSummary base = SubMsBench.summarizeLean(
                SubMsBench.runBench(new ExplicitRecipe(100, 200, 300, 400), SubMsBenchParams.defaults()));
        SubMsBenchSummary cand = SubMsBench.summarizeLean(
                SubMsBench.runBench(new ExplicitRecipe(110, 220, 330, 440), SubMsBenchParams.defaults()));
        SubMsBenchDiff diff = SubMsBench.diffSummary(base, cand);
        assertEquals(1, diff.stages().size());
        SubMsStageDiff put = diff.stages().get(0);
        assertEquals("put", put.stage());
        // Every metric should be 10% slower
        for (SubMsMetricDiff m : put.metrics()) {
            assertTrue(Math.abs(m.deltaPct() - 10.0) < 1e-9, m.metric() + " deltaPct=" + m.deltaPct());
        }
        assertTrue(Math.abs(put.worstRegressionPct() - 10.0) < 1e-9);
    }

    @Test
    void diffSummaryFlagsRegressionAboveThreshold() {
        SubMsBenchSummary base = SubMsBench.summarizeLean(
                SubMsBench.runBench(new ExplicitRecipe(100, 200, 300, 400), SubMsBenchParams.defaults()));
        SubMsBenchSummary cand = SubMsBench.summarizeLean(
                SubMsBench.runBench(new ExplicitRecipe(200, 400, 600, 800), SubMsBenchParams.defaults()));
        SubMsBenchDiff diff = SubMsBench.diffSummary(base, cand, 50.0);
        assertTrue(diff.hasRegression(), "100% regression vs 50% threshold");
        assertTrue(diff.worstStage().isPresent());
        assertEquals("put", diff.worstStage().get().stage());
    }

    @Test
    void diffSummaryDoesNotFlagWhenAllImproved() {
        SubMsBenchSummary base = SubMsBench.summarizeLean(
                SubMsBench.runBench(new ExplicitRecipe(200, 400, 600, 800), SubMsBenchParams.defaults()));
        SubMsBenchSummary cand = SubMsBench.summarizeLean(
                SubMsBench.runBench(new ExplicitRecipe(100, 200, 300, 400), SubMsBenchParams.defaults()));
        SubMsBenchDiff diff = SubMsBench.diffSummary(base, cand, 10.0);
        assertFalse(diff.hasRegression());
    }

    @Test
    void diffSummaryReportsStagesOnlyOnOneSide() {
        SubMsBenchSummary base = SubMsBench.summarizeLean(
                SubMsBench.runBench(new ExplicitRecipe(100, 200), SubMsBenchParams.defaults()));
        // Candidate has an extra stage by virtue of a custom recipe
        SubMsBenchSummary cand = SubMsBench.summarizeLean(
                SubMsBench.runBench(new SubMsRecipe() {
                    @Override public String name() { return "explicit"; }
                    @Override public void run(SubMsPerfHarness h, SubMsBenchParams p) {
                        h.stage("put", 2).record(100); h.stage("put", 2).record(200);
                        h.stage("get", 1).record(50);
                    }
                }, SubMsBenchParams.defaults()));
        SubMsBenchDiff diff = SubMsBench.diffSummary(base, cand);
        assertTrue(diff.candidateOnlyStages().contains("get"));
        assertTrue(diff.baselineOnlyStages().isEmpty());
    }

    @Test
    void printDiffEmitsTableWithVerdictColumn() {
        SubMsBenchSummary base = SubMsBench.summarizeLean(
                SubMsBench.runBench(new ExplicitRecipe(100, 200, 300, 400), SubMsBenchParams.defaults()));
        SubMsBenchSummary cand = SubMsBench.summarizeLean(
                SubMsBench.runBench(new ExplicitRecipe(200, 400, 600, 800), SubMsBenchParams.defaults()));
        SubMsBenchDiff diff = SubMsBench.diffSummary(base, cand, 50.0);
        java.io.ByteArrayOutputStream buf = new java.io.ByteArrayOutputStream();
        SubMsBench.printDiff(diff, new java.io.PrintStream(buf));
        String out = buf.toString(java.nio.charset.StandardCharsets.UTF_8);
        assertTrue(out.contains("stage"), out);
        assertTrue(out.contains("verdict"), out);
        assertTrue(out.contains("REGRESSED"), out);
    }

    @Test
    void diffToJsonEmitsExpectedKeys() throws Exception {
        SubMsBenchSummary base = SubMsBench.summarizeLean(
                SubMsBench.runBench(new ExplicitRecipe(100, 200, 300, 400), SubMsBenchParams.defaults()));
        SubMsBenchSummary cand = SubMsBench.summarizeLean(
                SubMsBench.runBench(new ExplicitRecipe(110, 220, 330, 440), SubMsBenchParams.defaults()));
        SubMsBenchDiff diff = SubMsBench.diffSummary(base, cand, 5.0);
        java.io.ByteArrayOutputStream buf = new java.io.ByteArrayOutputStream();
        SubMsBench.diffToJson(diff, new java.io.PrintStream(buf));
        String out = buf.toString(java.nio.charset.StandardCharsets.UTF_8);
        assertTrue(out.contains("\"has_regression\":true"), out);
        assertTrue(out.contains("\"stages\":["), out);
        assertTrue(out.contains("\"metric\":\"p99\""), out);
        assertTrue(out.contains("\"delta_pct\""), out);
    }
}
