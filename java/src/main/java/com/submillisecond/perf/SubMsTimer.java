package com.submillisecond.perf;

import java.io.PrintStream;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

/**
 * Zero-dep autostart stopwatch with named checkpoints / milestones. Lives in
 * the same crate as the bench harness so callers can drop one in
 * mid-loop without adding a dep.
 *
 * <pre>
 *   SubMsTimer t = new SubMsTimer("parse-request");
 *   t.mark("headers-read");
 *   handleHeaders();
 *   t.mark("body-decoded");
 *   handleBody();
 *   t.stop("served");
 *   t.print(System.out);
 *
 *   // Or grab structured data and ship it to your own pipeline:
 *   for (SubMsTimer.Checkpoint c : t.checkpoints()) {
 *       metrics.histogram(c.label()).record(c.elapsedSinceStartNs());
 *   }
 * </pre>
 *
 * <p>For production span/event work prefer
 * <a href="https://opentelemetry.io/docs/languages/java/">OpenTelemetry Java</a>
 * or Micrometer's {@code Timer.Sample} - they integrate with downstream
 * tracing backends. {@code SubMsTimer} is the right pick when you need
 * sub-microsecond overhead in a hot loop, you can't tolerate the OTel
 * exporter pipeline, and the consumer is the same JVM process.
 *
 * <p>The Rust crate ships an identical surface as {@code subms::SubMsTimer}.
 *
 * <p><b>Thread-safety:</b> instances are not thread-safe. Use one per task or
 * synchronise externally.
 */
public final class SubMsTimer {

    private final String name;
    private long startNs;
    private long lastNs;
    private long stoppedAtNs = -1;
    private final List<Checkpoint> checkpoints = new ArrayList<>();

    /** A single named milestone captured by {@link #mark} / {@link #lap} / {@link #stop}. */
    public record Checkpoint(
            /** Label passed to {@link #mark}, {@link #lap}, or {@link #stop}. */ String label,
            /** Nanoseconds between this checkpoint and the previous checkpoint
             *  (or {@link #start} if this is the first). */ long sinceLastNs,
            /** Nanoseconds between this checkpoint and {@link #start}. */ long sinceStartNs,
            /** Whether this checkpoint marked the timer as stopped. */ boolean isStop) {
    }

    /** Autostart unnamed timer. */
    public SubMsTimer() {
        this("");
    }

    /** Autostart timer with a display name (printed in the header). */
    public SubMsTimer(String name) {
        this.name = Objects.requireNonNullElse(name, "");
        start();
    }

    /** Reset to t=0 and clear all checkpoints. */
    public SubMsTimer start() {
        long now = System.nanoTime();
        this.startNs = now;
        this.lastNs = now;
        this.stoppedAtNs = -1;
        this.checkpoints.clear();
        return this;
    }

    /** Clear all checkpoints and reset elapsed back to zero. Alias of {@link #start}. */
    public SubMsTimer reset() {
        return start();
    }

    /** Record a checkpoint with the given label. Returns the elapsed-since-start ns. */
    public long mark(String label) {
        long now = System.nanoTime();
        long sinceStart = now - startNs;
        long sinceLast = now - lastNs;
        lastNs = now;
        checkpoints.add(new Checkpoint(label, sinceLast, sinceStart, /*isStop*/ false));
        return sinceStart;
    }

    /** Alias of {@link #mark} - emphasises "split / next leg" semantics. */
    public long lap(String label) {
        return mark(label);
    }

    /** Final checkpoint; the timer stops accumulating after this. */
    public long stop(String label) {
        long now = System.nanoTime();
        long sinceStart = now - startNs;
        long sinceLast = now - lastNs;
        lastNs = now;
        stoppedAtNs = now;
        checkpoints.add(new Checkpoint(label, sinceLast, sinceStart, /*isStop*/ true));
        return sinceStart;
    }

    /** {@code true} once {@link #stop} has been called since the last {@link #start} / {@link #reset}. */
    public boolean isStopped() {
        return stoppedAtNs >= 0;
    }

    /** Elapsed since start. Frozen at the value when {@link #stop} was called if stopped. */
    public long elapsedNs() {
        return (stoppedAtNs >= 0 ? stoppedAtNs : System.nanoTime()) - startNs;
    }

    public String name() { return name; }

    /** Unmodifiable view of recorded checkpoints in insertion order. */
    public List<Checkpoint> checkpoints() {
        return Collections.unmodifiableList(checkpoints);
    }

    /** Print a fixed-width timeline. Byte-equivalent to Rust's
     *  {@code subms::SubMsTimer::print}.
     *  <p>Layout:
     *  <pre>
     *  timer "parse-request"  total=3.2us
     *    headers-read         +1.1us       1.1us
     *    body-decoded         +800ns       1.9us
     *    served *             +1.3us       3.2us
     *  </pre>
     *  Columns: label (left, 18 wide), delta-since-previous (right, 8 wide),
     *  elapsed-since-start (right, 8 wide). A trailing {@code *} on the label
     *  marks the {@link #stop} checkpoint. */
    public void print(PrintStream out) {
        out.printf("timer \"%s\"  total=%s%n", name, SubMsBench.formatNs(elapsedNs()));
        for (Checkpoint c : checkpoints) {
            String label = c.isStop() ? c.label() + " *" : c.label();
            out.printf("  %-18s  +%8s   %8s%n",
                    label,
                    SubMsBench.formatNs(c.sinceLastNs()),
                    SubMsBench.formatNs(c.sinceStartNs()));
        }
    }
}
