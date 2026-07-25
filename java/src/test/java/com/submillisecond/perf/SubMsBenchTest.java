package com.submillisecond.perf;

import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SubMsBenchTest {

    // ---------- new in 0.4.0 ----------

    @Test
    void summarizeSkippingDropsLeadingSamples() {
        SubMsPerfHarness h = new SubMsPerfHarness("w", "java");
        SubMsPerfHarness.Stage st = h.stage("s", 100);
        for (int i = 0; i < 100; i++) st.record(i == 0 ? 1_000_000L : 100L);
        SubMsBenchSummary fullSummary = SubMsBench.summarize(h);
        SubMsBenchSummary trimmed = SubMsBench.summarizeSkipping(h, 5);
        long fullMax = fullSummary.stages().get(0).maxNs();
        long trimmedMax = trimmed.stages().get(0).maxNs();
        assertEquals(1_000_000L, fullMax, "full keeps the outlier");
        assertEquals(100L, trimmedMax, "trimmed drops it");
    }

    @Test
    void summarizeReportsStddevForNonTrivialSamples() {
        SubMsPerfHarness h = new SubMsPerfHarness("w", "java");
        SubMsPerfHarness.Stage st = h.stage("s", 4);
        for (long v : new long[] { 100, 200, 300, 400 }) st.record(v);
        SubMsBenchSummary s = SubMsBench.summarize(h);
        long stddev = s.stages().get(0).stddevNs();
        // population mean = 250; sample stddev = sqrt(((150^2 + 50^2 + 50^2 + 150^2) / 3)) = 129.099
        assertTrue(stddev >= 125L && stddev <= 135L, "stddev around 129: " + stddev);
    }

    @Test
    void summarizeReportsStddevZeroForSingleSample() {
        SubMsPerfHarness h = new SubMsPerfHarness("w", "java");
        SubMsPerfHarness.Stage st = h.stage("s", 1);
        st.record(42L);
        SubMsBenchSummary s = SubMsBench.summarize(h);
        assertEquals(0L, s.stages().get(0).stddevNs());
    }

    // -------- 0.5.0: summarizeWindowed --------

    @Test
    void summarizeWindowedSplitsIntoCorrectNumberOfBuckets() {
        SubMsPerfHarness h = new SubMsPerfHarness("many", "java");
        SubMsPerfHarness.Stage s = h.stage("op", 100);
        for (int i = 0; i < 100; i++) s.record((i + 1) * 100L);
        java.util.List<SubMsBenchSummary> windows = SubMsBench.summarizeWindowed(h, 20);
        assertEquals(5, windows.size(), "100 samples / 20-per-window = 5 windows");
        for (int i = 0; i < 5; i++) {
            assertEquals(Integer.toString(i), windows.get(i).inputs().get("__window_index"));
            assertEquals("20", windows.get(i).inputs().get("__window_size"));
        }
    }

    @Test
    void summarizeWindowedP99MonotonicForMonotonicInput() {
        SubMsPerfHarness h = new SubMsPerfHarness("mono", "java");
        SubMsPerfHarness.Stage s = h.stage("op", 100);
        for (int i = 0; i < 100; i++) s.record((i + 1) * 100L);
        java.util.List<SubMsBenchSummary> windows = SubMsBench.summarizeWindowed(h, 25);
        long prev = 0;
        for (SubMsBenchSummary w : windows) {
            long p99 = w.stages().get(0).p99Ns();
            assertTrue(p99 >= prev,
                    "p99 not monotonic across windows: " + prev + " -> " + p99);
            prev = p99;
        }
    }

    @Test
    void summarizeWindowedEmptyHarnessReturnsEmpty() {
        SubMsPerfHarness h = new SubMsPerfHarness("empty", "java");
        assertTrue(SubMsBench.summarizeWindowed(h, 10).isEmpty());
    }

    @Test
    void summarizeWindowedZeroWindowTreatedAsOne() {
        SubMsPerfHarness h = new SubMsPerfHarness("zero", "java");
        SubMsPerfHarness.Stage s = h.stage("op", 100);
        for (int i = 0; i < 100; i++) s.record(i);
        java.util.List<SubMsBenchSummary> windows = SubMsBench.summarizeWindowed(h, 0);
        assertEquals(100, windows.size(), "window 0 -> 1, 100 samples / 1 = 100 windows");
    }

    @Test
    void contendedWarmupRunsEveryIteration() {
        java.util.concurrent.atomic.AtomicInteger count = new java.util.concurrent.atomic.AtomicInteger();
        SubMsBench.contendedWarmup(4, 250, (tid, i) -> count.incrementAndGet());
        assertEquals(4 * 250, count.get());
    }

    @Test
    void contendedWarmupPropagatesSeenThreadIds() {
        java.util.Set<Integer> seen = java.util.Collections.synchronizedSet(new java.util.HashSet<>());
        SubMsBench.contendedWarmup(6, 5, (tid, i) -> seen.add(tid));
        assertEquals(6, seen.size(), "all thread ids should appear: " + seen);
    }

    @Test
    void diffSummaryReportsMetricRegression() {
        SubMsBenchSummary baseline = oneStageSummary("op", 100, 200, 300, 400, 250);
        SubMsBenchSummary candidate = oneStageSummary("op", 110, 220, 330, 440, 275);
        SubMsBenchDiff diff = SubMsBench.diffSummary(baseline, candidate);
        assertEquals(1, diff.stages().size());
        SubMsStageDiff sd = diff.stages().get(0);
        assertEquals("op", sd.stage());
        assertTrue(sd.worstRegressionPct() > 0.0, "candidate is slower: " + sd.worstRegressionPct());
    }

    @Test
    void diffSummaryFlagsMissingAndNewStages() {
        SubMsBenchSummary baseline = oneStageSummary("only_baseline", 100, 200, 300, 400, 250);
        SubMsBenchSummary candidate = oneStageSummary("only_candidate", 100, 200, 300, 400, 250);
        SubMsBenchDiff diff = SubMsBench.diffSummary(baseline, candidate);
        assertTrue(diff.baselineOnlyStages().contains("only_baseline"));
        assertTrue(diff.candidateOnlyStages().contains("only_candidate"));
    }

    @Test
    void diffToJsonContainsStageAndPercentageFields() throws Exception {
        SubMsBenchSummary baseline = oneStageSummary("op", 100, 200, 300, 400, 250);
        SubMsBenchSummary candidate = oneStageSummary("op", 200, 400, 600, 800, 500);
        SubMsBenchDiff diff = SubMsBench.diffSummary(baseline, candidate);
        java.io.ByteArrayOutputStream baos = new java.io.ByteArrayOutputStream();
        try (java.io.PrintStream ps = new java.io.PrintStream(baos, true, "UTF-8")) {
            SubMsBench.diffToJson(diff, ps);
        }
        String s = baos.toString("UTF-8");
        assertTrue(s.contains("\"stage\":\"op\""), "stage emitted: " + s.substring(0, Math.min(200, s.length())));
        assertTrue(s.contains("delta_pct"), "delta_pct emitted: " + s.substring(0, Math.min(200, s.length())));
    }

    @Test
    void printSummaryWritesHumanReadableHeader() {
        SubMsBenchSummary s = oneStageSummary("op", 100, 200, 300, 400, 250);
        java.io.ByteArrayOutputStream baos = new java.io.ByteArrayOutputStream();
        try (java.io.PrintStream ps = new java.io.PrintStream(baos, true, java.nio.charset.StandardCharsets.UTF_8)) {
            SubMsBench.printSummary(s, ps);
        }
        String out = baos.toString(java.nio.charset.StandardCharsets.UTF_8);
        assertTrue(out.contains("op"), "stage name emitted: " + out);
    }

    private static SubMsBenchSummary oneStageSummary(String stage, long p50, long p99, long p999, long max, long mean) {
        SubMsStageSummary ss = new SubMsStageSummary(stage, 100, p50, p99, p999, max, mean, 0L, java.util.Optional.empty());
        return new SubMsBenchSummary(
                "wl", "java",
                "2026-05-23T00:00:00Z",
                null,
                null,
                java.util.Map.of(),
                java.util.Map.of(),
                List.of(ss));
    }

    @Test
    void cpuPlacementInJson() throws Exception {
        SubMsStageSummary ss = new SubMsStageSummary("s", 1, 1, 1, 1, 1, 1, 0L, java.util.Optional.empty());
        SubMsBenchSummary withCpu = new SubMsBenchSummary(
                "wl", "java", "t", 1, "1", java.util.Map.of(), java.util.Map.of(), List.of(ss));
        java.io.StringWriter a = new java.io.StringWriter();
        SubMsBench.summaryToJson(withCpu, a);
        assertTrue(a.toString().contains("\"cpu\":{\"core\":1,\"affinity\":\"1\"}"), a.toString());
        SubMsBenchSummary noCpu = new SubMsBenchSummary(
                "wl", "java", "t", null, null, java.util.Map.of(), java.util.Map.of(), List.of(ss));
        java.io.StringWriter b = new java.io.StringWriter();
        SubMsBench.summaryToJson(noCpu, b);
        assertTrue(b.toString().contains("\"cpu\":null"), b.toString());
    }

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
        SubMsPerfHarness h = SubMsBench.runBench(new FixedRecipe(), new SubMsBenchParams(123, 45, 9L, 500));
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
    void downsampleRespectsCap() {
        long[] full = new long[2000];
        for (int i = 0; i < full.length; i++) full[i] = i;
        assertEquals(500, SubMsBench.downsample(full, 500).length);   // step 2000/500 = 4
        assertEquals(2000, SubMsBench.downsample(full, 5000).length); // cap >= len keeps all
        assertEquals(2000, SubMsBench.downsample(full, 2000).length);
        assertEquals(1, SubMsBench.downsample(full, 0).length);       // cap 0 -> 1
        assertEquals(0, SubMsBench.downsample(new long[0], 500).length);
    }

    @Test
    void runBenchThreadsSampleCap() {
        SubMsPerfHarness h = SubMsBench.runBench(new FixedRecipe(), new SubMsBenchParams(4, 0, 0L, 12_345));
        assertEquals(12_345, h.sampleCap());
        // Default params keep the 500 back-compat cap.
        assertEquals(500, SubMsBench.runBench(new FixedRecipe(), SubMsBenchParams.defaults()).sampleCap());
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
                        new SubMsBenchParams(4, 0, 0L, 500),
                        new SubMsBenchParams(4, 0, 1L, 500)),
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
                List.of(new SubMsBenchParams(4, 0, 0L, 500), new SubMsBenchParams(4, 0, 0L, 500)),
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
                List.of(new SubMsBenchParams(4, 0, 0L, 500), new SubMsBenchParams(4, 0, 0L, 500)),
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
