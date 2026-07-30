package com.submillisecond.perf;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.OptionalLong;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SubMsBenchConfigTest {

    @Test
    void newDefaultsCpuPinSingle() {
        SubMsBenchConfig c = new SubMsBenchConfig();
        assertEquals(SubMsCpuPin.SINGLE, c.cpuPin(), "absent cpu_pin -> Single");
        assertEquals(OptionalLong.empty(), c.cores());
        assertEquals(OptionalLong.empty(), c.sampleCap());
        assertNull(c.reason());
    }

    @Test
    void cpuPinWireTokensRoundTrip() {
        assertEquals(SubMsCpuPin.SINGLE, SubMsCpuPin.fromWire("single"));
        assertEquals(SubMsCpuPin.MULTI, SubMsCpuPin.fromWire("multi"));
        assertEquals(SubMsCpuPin.NONE, SubMsCpuPin.fromWire("none"));
        assertEquals("single", SubMsCpuPin.SINGLE.wire());
        assertEquals("multi", SubMsCpuPin.MULTI.wire());
        assertEquals("none", SubMsCpuPin.NONE.wire());
        // aliases + legacy boolean words map onto the canonical set.
        assertEquals(SubMsCpuPin.SINGLE, SubMsCpuPin.fromWire("ONE"));
        assertEquals(SubMsCpuPin.SINGLE, SubMsCpuPin.fromWire("true"));
        assertEquals(SubMsCpuPin.NONE, SubMsCpuPin.fromWire("false"));
        assertEquals(SubMsCpuPin.NONE, SubMsCpuPin.fromWire("off"));
        assertNull(SubMsCpuPin.fromWire("garbage"));
        assertNull(SubMsCpuPin.fromWire(null));
    }

    @Test
    void loadStrReadsTypedFields() {
        SubMsBenchConfig c = SubMsBenchConfig.loadStr(
                "{ \"cpu_pin\": \"multi\", \"cores\": 2, \"sample_cap\": 50000, \"reason\": \"multi-threaded\" }");
        assertEquals(SubMsCpuPin.MULTI, c.cpuPin());
        assertEquals(OptionalLong.of(2), c.cores());
        assertEquals(OptionalLong.of(50000), c.sampleCap());
        assertEquals("multi-threaded", c.reason());
    }

    @Test
    void legacyBooleanCpuPinStillReads() {
        assertEquals(SubMsCpuPin.SINGLE, SubMsBenchConfig.loadStr("{ \"cpu_pin\": true }").cpuPin());
        assertEquals(SubMsCpuPin.NONE, SubMsBenchConfig.loadStr("{ \"cpu_pin\": false }").cpuPin());
    }

    @Test
    void absentCpuPinDefaultsSingleWithOtherKeys() {
        SubMsBenchConfig c = SubMsBenchConfig.loadStr("{ \"sample_cap\": 500 }");
        assertEquals(SubMsCpuPin.SINGLE, c.cpuPin());
        assertEquals(OptionalLong.of(500), c.sampleCap());
    }

    @Test
    void malformedInputYieldsEmptyNeverThrows() {
        String[] bad = {
            null, "", "   ", "not json", "{", "[1,2,3]", "42", "null", "\"a string\"", "{\"cpu_pin\":"
        };
        for (String b : bad) {
            SubMsBenchConfig c = SubMsBenchConfig.loadStr(b);
            assertEquals(SubMsCpuPin.SINGLE, c.cpuPin(), "input <" + b + "> should default cpu_pin=Single");
            assertEquals(OptionalLong.empty(), c.cores());
        }
    }

    @Test
    void wrongTypedValuesFallBackToDefault() {
        SubMsBenchConfig c = SubMsBenchConfig.loadStr(
                "{ \"cpu_pin\": \"yes\", \"cores\": 2.5, \"sample_cap\": true, \"reason\": 7 }");
        assertEquals(SubMsCpuPin.SINGLE, c.cpuPin(), "unrecognised cpu_pin -> default Single");
        assertEquals(OptionalLong.empty(), c.cores(), "non-integer cores -> empty");
        assertEquals(OptionalLong.empty(), c.sampleCap(), "non-integer sample_cap -> empty");
        assertNull(c.reason(), "non-string reason -> null");
    }

    @Test
    void mergePreservesForeignFields() {
        String src = "{ \"sample_cap\": 50000, \"rounds\": 50,"
                + " \"storage\": { \"rounds\": 80, \"ops_per_round\": 2000 },"
                + " \"vendor\": { \"team\": \"risk\", \"ticket\": \"PERF-9\" }, \"cpu_pin\": \"single\" }";
        SubMsBenchConfig c = SubMsBenchConfig.loadStr(src);
        c.setCpuPin(SubMsCpuPin.MULTI).setCores(4);
        String out = c.toJson();
        for (String needle : new String[] {"rounds", "storage", "ops_per_round", "vendor", "PERF-9", "50000"}) {
            assertTrue(out.contains(needle), "foreign field " + needle + " lost in round-trip");
        }
        SubMsBenchConfig reloaded = SubMsBenchConfig.loadStr(out);
        assertEquals(SubMsCpuPin.MULTI, reloaded.cpuPin());
        assertEquals(OptionalLong.of(4), reloaded.cores());
        assertEquals(OptionalLong.of(50000), reloaded.sampleCap());
    }

    @Test
    void settersAreIdempotentAndPositional() {
        SubMsBenchConfig c = SubMsBenchConfig.loadStr("{ \"cpu_pin\": \"single\", \"cores\": 1 }");
        c.setCpuPin(SubMsCpuPin.NONE);
        c.setCpuPin(SubMsCpuPin.NONE); // second set must not duplicate the key
        c.setCores(8).setSampleCap(50000).setReason("r");
        String out = c.toJson();
        assertEquals(1, countOccurrences(out, "\"cpu_pin\""), "cpu_pin duplicated");
        assertEquals(1, countOccurrences(out, "\"cores\""));
        SubMsBenchConfig r = SubMsBenchConfig.loadStr(out);
        assertEquals(SubMsCpuPin.NONE, r.cpuPin());
        assertEquals(OptionalLong.of(8), r.cores());
        assertEquals(OptionalLong.of(50000), r.sampleCap());
        assertEquals("r", r.reason());
    }

    @Test
    void loadMissingThenSaveCreatesFileAndDirs(@TempDir Path dir) throws IOException {
        Path path = dir.resolve("nested").resolve(".subms").resolve("perf").resolve("controls.json");
        assertFalse(Files.exists(path));
        SubMsBenchConfig c = SubMsBenchConfig.load(path);
        assertEquals(SubMsCpuPin.SINGLE, c.cpuPin(), "missing file -> default");
        c.setCpuPin(SubMsCpuPin.MULTI).setCores(2);
        c.save(path);
        assertTrue(Files.exists(path));
        SubMsBenchConfig reloaded = SubMsBenchConfig.load(path);
        assertEquals(SubMsCpuPin.MULTI, reloaded.cpuPin());
        assertEquals(OptionalLong.of(2), reloaded.cores());
    }

    private static int countOccurrences(String haystack, String needle) {
        int n = 0;
        int i = 0;
        while ((i = haystack.indexOf(needle, i)) >= 0) {
            n++;
            i += needle.length();
        }
        return n;
    }
}
