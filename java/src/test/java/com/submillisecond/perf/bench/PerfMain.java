package com.submillisecond.perf.bench;

import com.submillisecond.perf.*;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.util.List;

/**
 * Self-bench. Measures the harness's own hot paths and emits a
 * {@link SubMsBenchSummary} on stdout. Driven by `subms-action-bench` in
 * this repo's perf workflow - if a future PR slows down the recording
 * overhead by more than the per-stage threshold, the gate catches it on
 * the PR.
 *
 * <p>Lives under {@code src/test/java/} so it does NOT ship in the
 * published jar. Run from the perf workflow as:
 * <pre>
 *   mvn -q test-compile
 *   java -cp target/test-classes:target/classes com.submillisecond.perf.bench.PerfMain &gt; perf.json
 * </pre>
 *
 * <p>Stages:
 * <ul>
 *   <li>{@code time_closure} - end-to-end cost of {@code stage.time(() -&gt; {})}. The floor for any user who instruments their hot path with {@code time()}.</li>
 *   <li>{@code record_ns} - just the long[] append ({@code stage.record(ns)}). Strict lower bound on per-sample bookkeeping.</li>
 *   <li>{@code summarize} - sort + percentile extraction over 50k samples. Cost of producing one summary.</li>
 *   <li>{@code summary_to_json} - serialisation cost paid by every CI run that uploads results.</li>
 *   <li>{@code diff_summary} - regression-diff math between two summaries. Cost paid by every PR-time gate.</li>
 * </ul>
 */
public final class PerfMain {

    private static final int SAMPLES_PER_STAGE = 50_000;

    public static void main(String[] args) throws Exception {
        SubMsPerfHarness h = new SubMsPerfHarness("subms-self-bench", "java");
        h.input("samples_per_stage", Integer.toString(SAMPLES_PER_STAGE));
        h.meta("java_version", System.getProperty("java.version"));
        h.meta("jar_version", "0.3.0");

        // 1. time_closure: cost of `stage.time(() -> {})` with an empty closure.
        {
            SubMsPerfHarness.Stage s = h.stage("time_closure", SAMPLES_PER_STAGE);
            for (int i = 0; i < SAMPLES_PER_STAGE; i++) {
                s.time(() -> blackhole());
            }
        }

        // 2. record_ns: cost of `stage.record(ns)` alone. No timer.
        {
            SubMsPerfHarness.Stage s = h.stage("record_ns", SAMPLES_PER_STAGE);
            for (long i = 0; i < SAMPLES_PER_STAGE; i++) {
                s.record(100 + (i & 63));
            }
        }

        // 3. summarize: sort + percentile extraction over 50k samples.
        {
            SubMsPerfHarness fixture = buildSampleHarness(SAMPLES_PER_STAGE);
            SubMsPerfHarness.Stage s = h.stage("summarize", 100);
            for (int i = 0; i < 100; i++) {
                s.time(() -> {
                    SubMsBenchSummary sum = SubMsBench.summarize(fixture);
                    blackhole(sum);
                });
            }
        }

        // 4. summary_to_json: serialisation cost.
        {
            SubMsBenchSummary fixture = SubMsBench.summarize(buildSampleHarness(SAMPLES_PER_STAGE));
            SubMsPerfHarness.Stage s = h.stage("summary_to_json", 100);
            for (int i = 0; i < 100; i++) {
                s.time(() -> {
                    try {
                        ByteArrayOutputStream buf = new ByteArrayOutputStream(64 * 1024);
                        SubMsBench.summaryToJson(fixture, new PrintStream(buf));
                        blackhole(buf);
                    } catch (Exception e) {
                        throw new RuntimeException(e);
                    }
                });
            }
        }

        // 5. diff_summary: regression-diff math.
        {
            SubMsBenchSummary base = SubMsBench.summarize(buildSampleHarness(SAMPLES_PER_STAGE));
            SubMsBenchSummary cand = SubMsBench.summarize(buildSampleHarness(SAMPLES_PER_STAGE));
            SubMsPerfHarness.Stage s = h.stage("diff_summary", 1_000);
            for (int i = 0; i < 1_000; i++) {
                s.time(() -> {
                    SubMsBenchDiff d = SubMsBench.diffSummary(base, cand);
                    blackhole(d);
                });
            }
        }

        // SubMsTimer cross-check - wall-clock checkpoint reporting.
        SubMsTimer t = new SubMsTimer("self-bench-wall");
        t.mark("stages-complete");
        t.stop("emitting-json");
        h.meta("self_bench_wall_ns", Long.toString(t.elapsedNs()));

        SubMsBench.summaryToJson(SubMsBench.summarize(h), System.out);
    }

    /** Synthetic fixture harness used as fodder for summarize / json / diff. */
    private static SubMsPerfHarness buildSampleHarness(int n) {
        SubMsPerfHarness h = new SubMsPerfHarness("fixture", "java");
        SubMsPerfHarness.Stage s = h.stage("fixture-stage", n);
        for (int i = 0; i < n; i++) {
            // Log-ish distribution: tail every 1000 samples to exercise sort.
            long v = (i % 1000 == 0) ? 10_000L + (i & 1023) : 100L + (i & 63);
            s.record(v);
        }
        return h;
    }

    /** Prevent the JIT eliminating measured work. */
    private static volatile Object SINK;
    private static void blackhole()        { SINK = ""; }
    private static void blackhole(Object o) { SINK = o; }
}
