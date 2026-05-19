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
}
