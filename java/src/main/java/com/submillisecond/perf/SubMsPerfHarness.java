package com.submillisecond.perf;

import java.io.IOException;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.time.Instant;
import java.util.Arrays;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;

/**
 * Tiny zero-dep perf harness. Records timed samples per stage. Analysis and
 * serialisation live in {@link SubMsBench} - this class only owns the raw
 * sample buffers and metadata.
 *
 * <p>Example:
 * <pre>
 *   SubMsPerfHarness h = new SubMsPerfHarness("lsm-tree", "java");
 *   h.input("entries", "50000");
 *   h.meta("sstables", "46");
 *
 *   Stage put = h.stage("put", 50_000);
 *   for (int i = 0; i &lt; 50_000; i++) put.time(() -> { ... });
 *
 *   SubMsBenchSummary s = SubMsBench.summarize(h);
 *   SubMsBench.printSummary(s, System.out);
 *   SubMsBench.summaryToJson(s, System.out);
 * </pre>
 */
public final class SubMsPerfHarness {

    private final String workload;
    private final String lang;
    private final Map<String, String> inputs = new LinkedHashMap<>();
    private final Map<String, String> meta   = new LinkedHashMap<>();
    private final Map<String, Stage>  stages = new LinkedHashMap<>();

    public SubMsPerfHarness(String workload, String lang) {
        this.workload = Objects.requireNonNull(workload, "workload");
        this.lang     = Objects.requireNonNull(lang,     "lang");
    }

    public SubMsPerfHarness input(String key, String value) { inputs.put(key, value); return this; }
    public SubMsPerfHarness meta(String key, String value)  { meta.put(key, value);   return this; }

    /** Create a new stage with sample-buffer capacity. */
    public Stage stage(String name, int capacity) {
        Stage s = new Stage(name, capacity);
        stages.put(name, s);
        return s;
    }

    public Stage stage(String name) {
        return stages.get(name);
    }

    /** Stages in registration order. */
    public java.util.Collection<Stage> stagesInOrder() {
        return stages.values();
    }

    public String workload() { return workload; }
    public String lang()     { return lang; }

    /** Unmodifiable view, registration order preserved. */
    public Map<String, String> inputs() { return Collections.unmodifiableMap(inputs); }
    /** Unmodifiable view, registration order preserved. */
    public Map<String, String> meta()   { return Collections.unmodifiableMap(meta); }

    /** ISO-8601 seconds-precision timestamp captured at call time, ending in {@code Z}.
     *  Matches the on-disk JSON's {@code timestamp} field. */
    public String timestamp() {
        return Instant.now().toString().substring(0, 19) + "Z";
    }

    /** Per-stage samples buffer + recorder. */
    public static final class Stage {
        private final String name;
        private long[] samples;
        private int n;

        Stage(String name, int capacity) {
            this.name = name;
            this.samples = new long[Math.max(16, capacity)];
        }

        /** Record an explicit duration in nanoseconds. */
        public void record(long ns) {
            if (n == samples.length) samples = Arrays.copyOf(samples, samples.length * 2);
            samples[n++] = ns;
        }

        /** Time a runnable and record its duration. */
        public void time(Runnable r) {
            long t0 = System.nanoTime();
            r.run();
            record(System.nanoTime() - t0);
        }

        /**
         * Wrap the stage with a paced recorder for coordinated-omission-corrected
         * benches. {@link PacedStage#time} blocks until each op's intended slot,
         * runs the workload, and records latency from the <em>intended</em> start
         * time (not the wall-clock start), which folds queue delay into the
         * per-op number - the correction Gil Tene's HdrHistogram exists for.
         *
         * <pre>
         *   PacedStage paced = stage.withPacing(10_000); // target 10k ops/sec
         *   for (int i = 0; i &lt; entries; i++) paced.time(() -&gt; doWork());
         * </pre>
         */
        public PacedStage withPacing(double targetOpsPerSecond) {
            return new PacedStage(this, targetOpsPerSecond);
        }

        public String name() { return name; }
        public int count() { return n; }

        /** Snapshot copy of the recorded samples, in chronological order. */
        public long[] samples() {
            return Arrays.copyOf(samples, n);
        }
    }

    /**
     * Coordinated-omission-corrected stage wrapper. Each {@link #time} call
     * blocks until its intended slot, runs the workload, then records the
     * latency from the <em>intended</em> start time to end-of-op (so queue
     * delay is reflected in the per-op latency, not silently dropped).
     *
     * <p>Use for benches that simulate constant-throughput arrivals - queues,
     * rate limiters, anything where "if the system stalls, late ops should
     * still count as slow". Has no effect on the loop scheduler if ops
     * complete in time; only kicks in when work overruns its slot.
     *
     * <p>Rust counterpart: {@code subms::PacedStage}.
     */
    public static final class PacedStage {
        private final Stage stage;
        private final long intervalNs;
        private final long startedAtNs;
        private long opIndex;

        PacedStage(Stage stage, double targetOpsPerSecond) {
            if (targetOpsPerSecond <= 0) {
                throw new IllegalArgumentException("targetOpsPerSecond must be > 0");
            }
            this.stage = stage;
            this.intervalNs = Math.max(1L, (long) (1_000_000_000.0 / targetOpsPerSecond));
            this.startedAtNs = System.nanoTime();
        }

        /** Time the runnable; latency is end-of-op minus <em>intended</em> start. */
        public void time(Runnable r) {
            long intendedStartNs = startedAtNs + opIndex * intervalNs;
            long now = System.nanoTime();
            if (now < intendedStartNs) {
                java.util.concurrent.locks.LockSupport.parkNanos(intendedStartNs - now);
            }
            r.run();
            long end = System.nanoTime();
            long correctedLatency = end - intendedStartNs;
            stage.record(correctedLatency);
            opIndex++;
        }

        /** Number of ops the wrapper has recorded so far. */
        public long opIndex() { return opIndex; }
        public long intervalNs() { return intervalNs; }
    }

    /** Back-compat: summarise + emit JSON in the standard subms JSON shape.
     *  New code should call {@link SubMsBench#summarize} then
     *  {@link SubMsBench#summaryToJson} so the analyser is explicit. */
    public void writeJson(PrintStream out) throws IOException {
        SubMsBench.summaryToJson(SubMsBench.summarize(this), out);
    }

    /** Parse stdin key=value lines into a map. Skips blank lines and `#` comments. */
    public static Map<String, String> readStdinKv() throws IOException {
        Map<String, String> m = new LinkedHashMap<>();
        java.io.BufferedReader br = new java.io.BufferedReader(new java.io.InputStreamReader(System.in, StandardCharsets.UTF_8));
        String line;
        while ((line = br.readLine()) != null) {
            String t = line.trim();
            if (t.isEmpty() || t.startsWith("#")) continue;
            int eq = t.indexOf('=');
            if (eq < 0) continue;
            m.put(t.substring(0, eq).trim(), t.substring(eq + 1).trim());
        }
        return m;
    }
}
