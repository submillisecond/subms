package com.submillisecond.perf;

import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SubMsTimerTest {

    @Test
    void autostartAndMarkCaptureIncreasingSinceStart() throws Exception {
        SubMsTimer t = new SubMsTimer("x");
        Thread.sleep(1);
        long a = t.mark("a");
        Thread.sleep(1);
        long b = t.mark("b");
        assertTrue(a > 0);
        assertTrue(b > a);
        List<SubMsTimer.Checkpoint> cs = t.checkpoints();
        assertEquals(2, cs.size());
        assertEquals("a", cs.get(0).label());
        assertEquals("b", cs.get(1).label());
        assertFalse(cs.get(0).isStop());
    }

    @Test
    void stopMarksIsStopAndFreezesElapsed() throws Exception {
        SubMsTimer t = new SubMsTimer("x");
        Thread.sleep(1);
        t.stop("done");
        assertTrue(t.isStopped());
        long e1 = t.elapsedNs();
        Thread.sleep(2);
        long e2 = t.elapsedNs();
        assertEquals(e1, e2);
        assertTrue(t.checkpoints().get(t.checkpoints().size() - 1).isStop());
    }

    @Test
    void resetClearsCheckpoints() {
        SubMsTimer t = new SubMsTimer("x");
        t.mark("a");
        t.mark("b");
        t.reset();
        assertTrue(t.checkpoints().isEmpty());
        assertFalse(t.isStopped());
    }

    @Test
    void lapIsAliasOfMark() {
        SubMsTimer t = new SubMsTimer("x");
        t.lap("a");
        assertEquals(1, t.checkpoints().size());
        assertEquals("a", t.checkpoints().get(0).label());
    }

    @Test
    void printEmitsHeaderAndCheckpoints() {
        SubMsTimer t = new SubMsTimer("parse");
        t.mark("a");
        t.stop("done");
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        t.print(new PrintStream(buf));
        String out = buf.toString(StandardCharsets.UTF_8);
        assertTrue(out.contains("timer \"parse\""));
        assertTrue(out.contains("a"));
        assertTrue(out.contains("done *"));
    }

    @Test
    void unnamedTimerDefaultsToEmptyName() {
        SubMsTimer t = new SubMsTimer();
        assertEquals("", t.name());
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        t.print(new PrintStream(buf));
        assertTrue(buf.toString(StandardCharsets.UTF_8).startsWith("timer \"\""));
    }

    @Test
    void sinceLastMeasuresDeltaBetweenMarks() throws Exception {
        SubMsTimer t = new SubMsTimer("x");
        Thread.sleep(1);
        t.mark("a");
        Thread.sleep(3);
        t.mark("b");
        SubMsTimer.Checkpoint b = t.checkpoints().get(1);
        assertTrue(b.sinceLastNs() >= 1_000_000L);   // at least ~1 ms slept
        assertTrue(b.sinceStartNs() > b.sinceLastNs());
    }

    @Test
    void elapsedRunningGrowsBeforeStop() throws Exception {
        SubMsTimer t = new SubMsTimer("x");
        long e1 = t.elapsedNs();
        Thread.sleep(2);
        long e2 = t.elapsedNs();
        assertTrue(e2 > e1);
    }

    // ---------------- static clock API ----------------

    @Test
    void nanosNowReturnsPositiveIncreasingValue() throws Exception {
        long a = SubMsTimer.nanosNow();
        Thread.sleep(1);
        long b = SubMsTimer.nanosNow();
        assertTrue(a >= 0, "nanosNow >= 0");
        assertTrue(b > a, "nanosNow monotonic: " + a + " -> " + b);
    }

    @Test
    void tickAndElapsedNsCapturePositiveInterval() throws Exception {
        SubMsTimer.SubMsTick t = SubMsTimer.tick();
        Thread.sleep(2);
        long ns = t.elapsedNs();
        assertTrue(ns >= 1_000_000L, "should be >= 1ms after Thread.sleep(2): " + ns);
        assertTrue(ns < 100_000_000L, "shouldn't be > 100ms on a healthy box: " + ns);
    }

    @Test
    void tickIsReusableForMultipleReads() throws Exception {
        SubMsTimer.SubMsTick t = SubMsTimer.tick();
        Thread.sleep(1);
        long a = t.elapsedNs();
        Thread.sleep(1);
        long b = t.elapsedNs();
        assertTrue(b >= a, "second read >= first: " + a + " -> " + b);
    }

    @Test
    void measureNsReturnsElapsedAndRunsRunnable() throws Exception {
        int[] sink = { 0 };
        long elapsed = SubMsTimer.measureNs(() -> {
            sink[0]++;
            try { Thread.sleep(1); } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
        });
        assertEquals(1, sink[0], "runnable should run exactly once");
        assertTrue(elapsed >= 500_000L, "elapsed should be at least 0.5ms: " + elapsed);
    }

    @Test
    void measureNsReturnsZeroOrPositiveForNoOp() {
        long elapsed = SubMsTimer.measureNs(() -> {});
        assertTrue(elapsed >= 0L, "no-op should be >= 0ns: " + elapsed);
        assertTrue(elapsed < 1_000_000L, "no-op shouldn't take >= 1ms: " + elapsed);
    }

    @Test
    void stopwatchUsesNanosNowSoMixingClockReadsIsCoherent() throws Exception {
        // mark() should report sinceStart consistent with an external
        // nanosNow() bracketing the timer's lifetime.
        long before = SubMsTimer.nanosNow();
        SubMsTimer t = new SubMsTimer("mix");
        Thread.sleep(1);
        long sinceStart = t.mark("m");
        long after = SubMsTimer.nanosNow();
        long bracket = after - before;
        // The bracket includes timer construction overhead, so sinceStart
        // must be <= bracket. Allows the equality case if both calls land
        // in the same QPC tick on Windows.
        assertTrue(sinceStart <= bracket, "sinceStart " + sinceStart + " > bracket " + bracket);
    }
}
