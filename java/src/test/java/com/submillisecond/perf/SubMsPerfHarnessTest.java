package com.submillisecond.perf;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Correctness tests for {@link SubMsPerfHarness}. Substring assertions on the
 * JSON output are intentional - the harness's output contract is exactly
 * "this textual shape" (consumed by the web app's perf chart loader),
 * not "any equivalent JSON". A round-trip parse would let a structural
 * change slip through, so we pin the literal shape and use diagnostic
 * messages on failure.
 */
final class SubMsPerfHarnessTest {

    @Test
    @DisplayName("writeJson emits the documented top-level shape")
    void writeJsonRoundTrips() throws Exception {
        SubMsPerfHarness h = new SubMsPerfHarness("toy", "java");
        h.input("entries", "1000");
        h.input("bloom_mode", "on");
        h.meta("sstables", "1");

        SubMsPerfHarness.Stage s = h.stage("work", 1000);
        for (int i = 0; i < 1000; i++) s.record(i * 10L);

        String json = renderJson(h);

        assertTrue(json.startsWith("{"),                                  () -> "json starts with `{`: " + head(json));
        assertTrue(json.contains("\"workload\":\"toy\""),                  () -> "workload field present: " + head(json));
        assertTrue(json.contains("\"lang\":\"java\""),                     () -> "lang field present: " + head(json));
        assertTrue(json.contains("\"entries\":\"1000\""),                  () -> "input passthrough: " + head(json));
        assertTrue(json.contains("\"sstables\":\"1\""),                    () -> "meta passthrough: " + head(json));
        assertTrue(json.contains("\"work\":{"),                            () -> "stage emitted: " + head(json));
        assertTrue(json.contains("\"count\":1000"),                        () -> "count emitted: " + head(json));
        assertTrue(json.contains("\"samples_ns\":["),                      () -> "samples_ns array emitted: " + head(json));
    }

    @Test
    @DisplayName("percentile fields land where the sample distribution puts them")
    void percentilesMatchSamples() throws Exception {
        // samples 0..99 -> p50 == 50, max == 99 (the harness uses ceiling-index
        // percentile per the documented JSON contract).
        SubMsPerfHarness h = new SubMsPerfHarness("perc", "java");
        SubMsPerfHarness.Stage s = h.stage("op", 100);
        for (int i = 0; i < 100; i++) s.record(i);

        String json = renderJson(h);

        assertTrue(json.contains("\"p50_ns\":50"),  () -> "p50 == 50: " + head(json));
        assertTrue(json.contains("\"max_ns\":99"),  () -> "max == 99: " + head(json));
    }

    @Test
    @DisplayName("multiple stages serialise in declaration order")
    void multipleStagesPreserveOrder() throws Exception {
        SubMsPerfHarness h = new SubMsPerfHarness("multi", "java");
        h.stage("put",  10).record(1);
        h.stage("get",  10).record(2);
        h.stage("scan", 10).record(3);

        String json = renderJson(h);
        int put  = json.indexOf("\"put\":{");
        int get  = json.indexOf("\"get\":{");
        int scan = json.indexOf("\"scan\":{");
        assertTrue(put  >= 0 && get >= 0 && scan >= 0,
                () -> "all three stages present: " + head(json));
        assertTrue(put < get && get < scan,
                () -> "stages emitted in declaration order: put=" + put + " get=" + get + " scan=" + scan);
    }

    @Test
    @DisplayName("workload and lang are required at construction")
    void requiresWorkloadAndLang() {
        // Defensive sanity - downstream tooling keys on these fields and a
        // null/empty value would silently propagate into the JSON.
        assertThrows(NullPointerException.class, () -> new SubMsPerfHarness(null, "java"));
        assertThrows(NullPointerException.class, () -> new SubMsPerfHarness("toy", null));
    }

    @Test
    @DisplayName("stage().time() captures wall time and feeds the histogram")
    void stageTimeCaptures() throws Exception {
        SubMsPerfHarness h = new SubMsPerfHarness("time", "java");
        SubMsPerfHarness.Stage s = h.stage("noop", 5);
        for (int i = 0; i < 5; i++) {
            s.time(() -> {});            // measured no-op; nanos > 0
        }

        String json = renderJson(h);
        assertTrue(json.contains("\"count\":5"),         () -> "count from time() == 5: " + head(json));
        assertNotNull(json,                               "writeJson produced output");
    }

    private static String renderJson(SubMsPerfHarness h) throws Exception {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        try (PrintStream ps = new PrintStream(baos, true, "UTF-8")) {
            h.writeJson(ps);
        }
        return baos.toString("UTF-8");
    }

    @Test
    @DisplayName("PacedStage paces ops and folds queue delay into latency")
    void pacedStageCoordinatedOmissionCorrection() {
        SubMsPerfHarness h = new SubMsPerfHarness("paced", "java");
        // Target a slow rate so each op has a clear intended slot.
        SubMsPerfHarness.PacedStage paced = h.stage("op", 8).withPacing(1_000.0); // 1 ms / op

        // First op completes instantly; subsequent op is fired 2 ms late
        // (simulating a stall) - its corrected latency should reflect the slot delay.
        paced.time(() -> {});
        java.util.concurrent.locks.LockSupport.parkNanos(2_000_000L);   // 2 ms stall
        paced.time(() -> {});

        long[] samples = h.stage("op").samples();
        assertEquals(2, samples.length);
        // First op finished close to its slot - well under the 1 ms interval.
        // Loose bound on Windows where nanoTime() granularity + JIT-cold-call
        // overhead can push an empty lambda to several hundred microseconds.
        assertTrue(samples[0] < 1_000_000L, "first op below slot interval: " + samples[0]);
        // Second op's corrected latency should be ~2 ms (the slot delay) -
        // a coordinate-omission-uncorrected bench would record ~0 here.
        assertTrue(samples[1] > 1_000_000L, "second op reflects 2ms slot delay: " + samples[1]);
        // The point of CO correction is that second > first by ~1ms.
        assertTrue(samples[1] > samples[0] + 500_000L,
                "second op reflects extra slot delay over first: first=" + samples[0] + " second=" + samples[1]);
    }

    /** Short prefix of the JSON for assertion messages; full body would be unreadable. */
    private static String head(String s) {
        return s.length() < 240 ? s : s.substring(0, 240) + "...";
    }
}
