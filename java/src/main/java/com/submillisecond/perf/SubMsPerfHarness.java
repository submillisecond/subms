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
    // volatile so an observer registered on one thread is visible to a
    // recorder running on another (the harness can be shared across threads
    // for contended-stage benches via `contendedWarmup`).
    private volatile SubMsObserver observer = null;
    // Max points kept in each stage's emitted samples_ns timeline. Default 500;
    // SubMsBench.runBench sets it from SubMsBenchParams.sampleCap(). Mirrors the
    // Rust harness's sample_cap.
    private int sampleCap = 500;

    public SubMsPerfHarness(String workload, String lang) {
        this.workload = Objects.requireNonNull(workload, "workload");
        this.lang     = Objects.requireNonNull(lang,     "lang");
        stampRuntimeMeta();
    }

    /**
     * Record the JVM facts that change what a Java number MEANS.
     *
     * <p>Stamped by the harness rather than by each recipe, because it is the
     * same for every capture and a recipe that forgets produces a number nobody
     * can situate. {@code jvm_heap_max} matters more than it looks: the same
     * bench under a 256 MB and a 700 MB ceiling are different experiments, and a
     * GC-bound tail is invisible in the percentiles that come out of it. Anything
     * a recipe sets afterwards wins - these are defaults, not a lock.
     */
    private void stampRuntimeMeta() {
        meta.put("java_version", System.getProperty("java.version", ""));
        long maxMb = Runtime.getRuntime().maxMemory() / (1024L * 1024L);
        meta.put("jvm_heap_max", maxMb + "m");
        meta.put("jvm_vm", System.getProperty("java.vm.name", ""));
    }

    /** Set the samples_ns downsample cap (clamped to at least 1). Returns this. */
    public SubMsPerfHarness sampleCap(int cap) { this.sampleCap = Math.max(1, cap); return this; }

    /** The configured samples_ns downsample cap (default 500). */
    public int sampleCap() { return sampleCap; }

    public SubMsPerfHarness input(String key, String value) { inputs.put(key, value); return this; }
    public SubMsPerfHarness meta(String key, String value)  { meta.put(key, value);   return this; }

    /** Register an observer to receive every recorded sample and the
     *  post-bench summary. Replaces any existing observer. Returns this for
     *  builder-style chaining. The sibling {@code subms-otel} library
     *  ships {@code OtelObserver} / {@code OtelObserverAsync} / etc. */
    public SubMsPerfHarness withObserver(SubMsObserver observer) {
        this.observer = observer;
        return this;
    }

    /** Mutable setter form of {@link #withObserver(SubMsObserver)}. Pass
     *  {@code null} to clear. */
    public SubMsPerfHarness setObserver(SubMsObserver observer) {
        this.observer = observer;
        return this;
    }

    /** The currently-registered observer, or {@code null} if none. Mostly
     *  for tests; benches don't need to read this. */
    public SubMsObserver observer() { return observer; }

    /** Create a new stage with sample-buffer capacity. */
    public Stage stage(String name, int capacity) {
        Stage s = new Stage(this, name, capacity);
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

    /** Per-stage samples buffer + recorder. Optionally annotated with a
     *  {@link SubMsStageKind} that sibling adapters (e.g. {@code subms-otel})
     *  use to pick histogram bucket boundaries. */
    public static final class Stage {
        // Back-reference to the harness so each recorded sample can read the
        // currently-registered observer + build the SubMsObservationCtx
        // without copying workload/lang onto every stage.
        private final SubMsPerfHarness harness;
        private final String name;
        private long[] samples;
        private int n;
        private SubMsStageKind kind = SubMsStageKind.UNSPECIFIED;

        Stage(SubMsPerfHarness harness, String name, int capacity) {
            this.harness = harness;
            this.name = name;
            this.samples = new long[Math.max(16, capacity)];
        }

        /** Annotate this stage's kind so observers can pick fitting histogram
         *  buckets. Default is {@link SubMsStageKind#UNSPECIFIED}. Chainable. */
        public Stage withKind(SubMsStageKind kind) {
            this.kind = kind;
            return this;
        }

        public SubMsStageKind kind() { return kind; }

        /** Record an explicit duration in nanoseconds. Also fires the
         *  harness's observer (if registered). */
        public void record(long ns) {
            if (n == samples.length) samples = Arrays.copyOf(samples, samples.length * 2);
            samples[n++] = ns;
            SubMsObserver obs = harness.observer;
            if (obs != null) {
                obs.onRecord(
                        new SubMsObservationCtx(harness.workload, harness.lang, name, kind),
                        ns);
            }
        }

        /** Time a runnable and record its duration. */
        public void time(Runnable r) {
            long t0 = SubMsTimer.nanosNow();
            r.run();
            record(SubMsTimer.nanosNow() - t0);
        }

        /**
         * Warm the JIT, then record {@code measured} timed samples of
         * {@code op}. Runs {@code op} for {@code warmup} untimed iterations
         * first so HotSpot promotes the hot path to C2 - and escape analysis
         * can elide short-lived allocations - before any sample is recorded.
         *
         * <p>Without this, the first several thousand JVM invocations run in
         * the interpreter or C1, which inflates p99 by one to three orders of
         * magnitude on low-iteration stages: a single un-warmed pass over a
         * structure can read as milliseconds where the steady state is tens of
         * microseconds. The Rust harness needs no equivalent because it is
         * AOT-compiled - even the first call runs optimised machine code. Java
         * benches that want numbers comparable to the Rust side must warm.
         *
         * <p>Use for fixed-op stages - a merge pass, a snapshot capture, a
         * tick. For stages whose operation varies per iteration, see
         * {@link #warmThenTime(int, int, java.util.function.IntConsumer)}.
         */
        public void warmThenTime(int warmup, int measured, Runnable op) {
            for (int i = 0; i < warmup; i++) op.run();
            for (int i = 0; i < measured; i++) time(op);
        }

        /**
         * Index-aware variant of {@link #warmThenTime(int, int, Runnable)} for
         * stages whose operation varies per iteration (for example adding
         * {@code keys[i]}). {@code op} receives the iteration index for both
         * the untimed warmup pass ({@code 0..warmup}) and the timed pass
         * ({@code 0..measured}); index into a shorter input stream with
         * {@code i % len}.
         */
        public void warmThenTime(int warmup, int measured, java.util.function.IntConsumer op) {
            for (int i = 0; i < warmup; i++) op.accept(i);
            for (int i = 0; i < measured; i++) {
                long t0 = SubMsTimer.nanosNow();
                op.accept(i);
                record(SubMsTimer.nanosNow() - t0);
            }
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
            this.startedAtNs = SubMsTimer.nanosNow();
        }

        /** Time the runnable; latency is end-of-op minus <em>intended</em> start. */
        public void time(Runnable r) {
            long intendedStartNs = startedAtNs + opIndex * intervalNs;
            long now = SubMsTimer.nanosNow();
            if (now < intendedStartNs) {
                java.util.concurrent.locks.LockSupport.parkNanos(intendedStartNs - now);
            }
            r.run();
            long end = SubMsTimer.nanosNow();
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
