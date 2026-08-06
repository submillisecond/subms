package com.submillisecond.perf;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
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

    @Test
    @DisplayName("PacedStage exposes opIndex + intervalNs after recording")
    void pacedStageAccessorsTrackProgress() {
        SubMsPerfHarness h = new SubMsPerfHarness("p", "java");
        SubMsPerfHarness.PacedStage paced = h.stage("op", 4).withPacing(2_000.0); // 500us / op
        assertEquals(0L, paced.opIndex());
        assertEquals(500_000L, paced.intervalNs());
        paced.time(() -> {});
        assertEquals(1L, paced.opIndex());
    }

    @Test
    @DisplayName("PacedStage rejects non-positive target rate")
    void pacedStageRejectsBadRate() {
        SubMsPerfHarness h = new SubMsPerfHarness("p", "java");
        SubMsPerfHarness.Stage s = h.stage("op", 1);
        assertThrows(IllegalArgumentException.class, () -> s.withPacing(0.0));
        assertThrows(IllegalArgumentException.class, () -> s.withPacing(-100.0));
    }

    @Test
    @DisplayName("inputs() + meta() return live maps with the recorded keys")
    void inputsAndMetaPropagated() {
        SubMsPerfHarness h = new SubMsPerfHarness("im", "java");
        h.input("entries", "1000");
        h.input("seed", "42");
        h.meta("host", "ci-1");
        assertEquals("1000", h.inputs().get("entries"));
        assertEquals("42", h.inputs().get("seed"));
        assertEquals("ci-1", h.meta().get("host"));
    }

    @Test
    @DisplayName("timestamp is iso-8601 UTC when set or captured")
    void timestampPresentAndIso() {
        SubMsPerfHarness h = new SubMsPerfHarness("t", "java");
        String ts = h.timestamp();
        assertNotNull(ts);
        assertTrue(ts.endsWith("Z"), "iso-8601 Z suffix: " + ts);
        assertTrue(ts.length() >= 19, "iso-8601 length: " + ts);
    }

    @Test
    @DisplayName("warmThenTime records only the measured pass, not the warmup")
    void warmThenTimeRecordsMeasuredOnly() {
        SubMsPerfHarness h = new SubMsPerfHarness("warm", "java");
        SubMsPerfHarness.Stage s = h.stage("op", 8);
        int[] calls = {0};
        s.warmThenTime(100, 8, () -> calls[0]++);
        // op ran warmup + measured times, but only the measured pass is sampled.
        assertEquals(108, calls[0], "op ran for warmup + measured iterations");
        assertEquals(8, s.count(), "only the measured pass produced samples");
    }

    @Test
    @DisplayName("warmThenTime(IntConsumer) passes the iteration index through both passes")
    void warmThenTimeIndexAware() {
        SubMsPerfHarness h = new SubMsPerfHarness("warm-idx", "java");
        SubMsPerfHarness.Stage s = h.stage("op", 5);
        int[] warmMax = {-1};
        int[] timedMax = {-1};
        // warmup 3, measured 5: warmup sees indices 0..2, timed sees 0..4.
        s.warmThenTime(3, 5, (int i) -> {
            // The timed pass is the longer one, so the final observed index is 4.
            if (i > timedMax[0]) timedMax[0] = i;
            if (i <= 2 && i > warmMax[0]) warmMax[0] = i;
        });
        assertEquals(5, s.count(), "measured-pass samples recorded");
        assertEquals(4, timedMax[0], "timed pass walks 0..measured-1");
        assertTrue(warmMax[0] >= 0, "warmup pass ran with its own index range");
    }

    @Test
    @DisplayName("stage(name) re-lookup returns the same Stage object")
    void stageReLookupReturnsSame() {
        SubMsPerfHarness h = new SubMsPerfHarness("r", "java");
        SubMsPerfHarness.Stage created = h.stage("op", 1);
        SubMsPerfHarness.Stage found = h.stage("op");
        assertTrue(created == found, "stage('op') after create returns the same instance");
    }

    // ---------- SubMsObserver integration -----------------------------------

    /** One captured record. */
    private record Captured(String stage, long ns, SubMsStageKind kind,
                            String workload, String lang) {}

    /** Test observer that records every call. */
    private static final class RecordingObserver implements SubMsObserver {
        final java.util.List<Captured> records = new java.util.concurrent.CopyOnWriteArrayList<>();
        final java.util.concurrent.atomic.AtomicInteger summaries = new java.util.concurrent.atomic.AtomicInteger();
        @Override public void onRecord(SubMsObservationCtx ctx, long ns) {
            records.add(new Captured(ctx.stage(), ns, ctx.stageKind(), ctx.workload(), ctx.lang()));
        }
        @Override public void onSummarize(SubMsBenchSummary summary) {
            summaries.incrementAndGet();
        }
    }

    @Test
    @DisplayName("observer is null by default and the harness behaves unchanged")
    void observerDefaultNoopDoesNotChangeBehaviour() {
        SubMsPerfHarness h = new SubMsPerfHarness("noop", "java");
        assertNull(h.observer(), "default observer is null");
        SubMsPerfHarness.Stage s = h.stage("op", 4);
        s.record(100);
        s.record(200);
        assertEquals(2, s.count());
        // Build a summary; no observer means no callback, no panic.
        SubMsBench.summarize(h);
    }

    @Test
    @DisplayName("observer fires on record() and time() with correct ctx")
    void observerFiresOnRecordAndTime() {
        RecordingObserver obs = new RecordingObserver();
        SubMsPerfHarness h = new SubMsPerfHarness("rec", "java").withObserver(obs);
        SubMsPerfHarness.Stage s = h.stage("op", 4);
        s.record(42);
        s.time(() -> {});
        assertEquals(2, obs.records.size(), "record + time each fire once");
        Captured first = obs.records.get(0);
        assertEquals("op", first.stage());
        assertEquals(42L, first.ns());
        assertEquals("rec", first.workload());
        assertEquals("java", first.lang());
        assertEquals("op", obs.records.get(1).stage());
    }

    @Test
    @DisplayName("observer fires on warmThenTime only for the measured pass")
    void observerFiresOnWarmThenTimeOnlyForMeasuredPass() {
        RecordingObserver obs = new RecordingObserver();
        SubMsPerfHarness h = new SubMsPerfHarness("warm", "java").withObserver(obs);
        SubMsPerfHarness.Stage s = h.stage("op", 8);
        s.warmThenTime(50, 8, () -> {});
        assertEquals(8, obs.records.size(), "observer fires only on the measured pass");
    }

    @Test
    @DisplayName("observer records carry the declared SubMsStageKind")
    void observerRecordsCarryDeclaredStageKind() {
        RecordingObserver obs = new RecordingObserver();
        SubMsPerfHarness h = new SubMsPerfHarness("kind", "java").withObserver(obs);
        h.stage("put", 4).withKind(SubMsStageKind.HOT_PATH).record(10);
        h.stage("compact", 4).withKind(SubMsStageKind.BATCH_OP).record(20);
        assertEquals(SubMsStageKind.HOT_PATH, obs.records.get(0).kind());
        assertEquals(SubMsStageKind.BATCH_OP, obs.records.get(1).kind());
    }

    @Test
    @DisplayName("onSummarize fires exactly once per summarize() call")
    void observerFiresOnSummarizeExactlyOnce() {
        RecordingObserver obs = new RecordingObserver();
        SubMsPerfHarness h = new SubMsPerfHarness("sum", "java").withObserver(obs);
        SubMsPerfHarness.Stage s = h.stage("op", 4);
        s.record(1);
        s.record(2);
        SubMsBench.summarize(h);
        assertEquals(1, obs.summaries.get());
    }

    @Test
    @DisplayName("setObserver after stages are created is honoured (Stage reads harness.observer per call)")
    void setObserverAfterStageCreationIsHonoured() {
        // Stage is created BEFORE the observer is installed; records taken
        // BEFORE installation are silent, but records taken after see the
        // newly-installed observer because Stage.record reads harness.observer
        // each call.
        SubMsPerfHarness h = new SubMsPerfHarness("late", "java");
        SubMsPerfHarness.Stage s = h.stage("op", 4);
        s.record(1); // silent
        RecordingObserver obs = new RecordingObserver();
        h.setObserver(obs);
        s.record(2);
        assertEquals(1, obs.records.size(), "only the post-install record fires");
        assertEquals(2L, obs.records.get(0).ns());
    }

    @Test
    void everyHarnessStampsTheJvmFactsThatChangeWhatANumberMeans() {
        // The same bench under a 256 MB and a 700 MB ceiling are different
        // experiments, and a GC-bound tail does not announce itself in the
        // percentiles. Stamped by the harness so a recipe cannot forget.
        SubMsPerfHarness h = new SubMsPerfHarness("w", "java");
        assertTrue(h.meta().get("jvm_heap_max").endsWith("m"), h.meta().toString());
        assertFalse(h.meta().get("java_version").isEmpty());
    }

    @Test
    void aRecipeCanOverrideTheStampedRuntimeMeta() {
        SubMsPerfHarness h = new SubMsPerfHarness("w", "java");
        h.meta("jvm_heap_max", "700m");
        assertEquals("700m", h.meta().get("jvm_heap_max"));
    }

    /**
     * The version fallback is a hand-maintained constant, so it can drift from the
     * pom on any release. Reading the pom here turns that from a silent wrong
     * number in every published capture into a failing build.
     */
    @Test
    void harnessVersionConstantMatchesThePom() throws Exception {
        String pom = java.nio.file.Files.readString(java.nio.file.Path.of("pom.xml"));
        java.util.regex.Matcher m = java.util.regex.Pattern
                .compile("<artifactId>subms</artifactId>\\s*<version>([^<]+)</version>")
                .matcher(pom);
        org.junit.jupiter.api.Assertions.assertTrue(m.find(), "could not read the project version from pom.xml");
        org.junit.jupiter.api.Assertions.assertEquals(
                m.group(1), SubMsPerfHarness.HARNESS_VERSION,
                "SubMsPerfHarness.HARNESS_VERSION is stale - bump it with the pom");
    }

    @Test
    void everyCaptureNamesTheHarnessThatTookIt() {
        SubMsPerfHarness h = new SubMsPerfHarness("w", "java");
        org.junit.jupiter.api.Assertions.assertEquals(
                SubMsPerfHarness.HARNESS_VERSION, h.meta().get("harness_version"));
    }
}
