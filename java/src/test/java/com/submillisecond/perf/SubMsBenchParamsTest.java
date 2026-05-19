package com.submillisecond.perf;

import org.junit.jupiter.api.Test;

import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SubMsBenchParamsTest {

    private static Map<String, String> m(String... pairs) {
        Map<String, String> map = new java.util.LinkedHashMap<>();
        for (int i = 0; i < pairs.length; i += 2) map.put(pairs[i], pairs[i + 1]);
        return map;
    }

    @Test
    void defaultsAreSensible() {
        SubMsBenchParams d = SubMsBenchParams.defaults();
        assertEquals(50_000, d.entries());
        assertEquals(5_000, d.warmup());
        assertEquals(0L, d.seed());
    }

    @Test
    void fromMapUsesDefaultsWhenMissing() {
        SubMsBenchParams p = SubMsBenchParams.fromMap(m());
        SubMsBenchParams d = SubMsBenchParams.defaults();
        assertEquals(d.entries(), p.entries());
        assertEquals(d.warmup(), p.warmup());
        assertEquals(d.seed(), p.seed());
    }

    @Test
    void fromMapReadsOverrides() {
        SubMsBenchParams p = SubMsBenchParams.fromMap(m("entries", "1000", "warmup", "100", "seed", "42"));
        assertEquals(1000, p.entries());
        assertEquals(100, p.warmup());
        assertEquals(42L, p.seed());
    }

    @Test
    void fromMapIgnoresGarbage() {
        SubMsBenchParams p = SubMsBenchParams.fromMap(m("entries", "abc", "warmup", "?", "seed", "nope"));
        SubMsBenchParams d = SubMsBenchParams.defaults();
        assertEquals(d.entries(), p.entries());
        assertEquals(d.warmup(), p.warmup());
        assertEquals(d.seed(), p.seed());
    }

    @Test
    void parseIntPresent() {
        assertEquals(1234, SubMsBenchParams.parseInt(m("n", "1234"), "n", 0));
    }

    @Test
    void parseIntMissing() {
        assertEquals(42, SubMsBenchParams.parseInt(m("other", "1"), "n", 42));
    }

    @Test
    void parseIntInvalidFallsBack() {
        assertEquals(42, SubMsBenchParams.parseInt(m("n", "abc"), "n", 42));
    }

    @Test
    void parseLongPresent() {
        assertEquals(99L, SubMsBenchParams.parseLong(m("seed", "99"), "seed", 0L));
    }

    @Test
    void parseLongMissing() {
        assertEquals(7L, SubMsBenchParams.parseLong(m(), "seed", 7L));
    }

    @Test
    void parseLongInvalidFallsBack() {
        assertEquals(7L, SubMsBenchParams.parseLong(m("seed", "not-a-number"), "seed", 7L));
    }

    @Test
    void parseStringPresent() {
        assertEquals("on", SubMsBenchParams.parseString(m("mode", "on"), "mode", "off"));
    }

    @Test
    void parseStringMissing() {
        assertEquals("off", SubMsBenchParams.parseString(m(), "mode", "off"));
    }

    @Test
    void parseBoolAcceptsTruthy() {
        for (String v : new String[]{"1", "true", "TRUE", "on", "On", "yes", "Yes"}) {
            assertTrue(SubMsBenchParams.parseBool(m("x", v), "x", false), v + " should be true");
        }
    }

    @Test
    void parseBoolAcceptsFalsy() {
        for (String v : new String[]{"0", "false", "off", "no", "NO"}) {
            assertFalse(SubMsBenchParams.parseBool(m("x", v), "x", true), v + " should be false");
        }
    }

    @Test
    void parseBoolUnknownFallsBack() {
        assertTrue(SubMsBenchParams.parseBool(m("x", "maybe"), "x", true));
        assertFalse(SubMsBenchParams.parseBool(m("x", "maybe"), "x", false));
    }

    @Test
    void parseBoolMissingFallsBack() {
        assertTrue(SubMsBenchParams.parseBool(m(), "x", true));
        assertFalse(SubMsBenchParams.parseBool(m(), "x", false));
    }
}
