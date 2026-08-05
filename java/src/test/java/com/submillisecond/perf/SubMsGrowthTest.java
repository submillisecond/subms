package com.submillisecond.perf;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.StringWriter;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;

/** Mirrors the Rust growth_tests.rs one for one, plus the shared wire fixture. */
class SubMsGrowthTest {

    /**
     * A recipe whose per-round footprint is scripted, so the verdict logic is
     * exercised deterministically. Mirror of the Rust ScriptedRecipe.
     */
    private static final class ScriptedRecipe implements SubMsGrowthRecipe {
        private final long[] disk;
        private final long live;
        private final SubMsGrowth.GrowthClass cls;
        private final double bound;
        private int round;

        ScriptedRecipe(long[] disk, long live, SubMsGrowth.GrowthClass cls, double bound) {
            this.disk = disk;
            this.live = live;
            this.cls = cls;
            this.bound = bound;
        }

        @Override public String name() {
            return "scripted";
        }

        @Override public int rounds() {
            return disk.length;
        }

        @Override public int opsPerRound() {
            return 4;
        }

        @Override public void op(int round, int i) {
            this.round = round;
        }

        @Override public long diskBytes() {
            return disk[round - 1];
        }

        @Override public long liveBytes() {
            return live;
        }

        @Override public Map<String, Long> structures() {
            return Map.of("sstables", (long) round);
        }

        @Override public SubMsGrowth.GrowthClass expectedClass() {
            return cls;
        }

        @Override public double expectedBound() {
            return bound;
        }
    }

    @Test
    void amplificationBoundedHoldsWhenDiskTracksLive() {
        var r = new ScriptedRecipe(new long[] {1000, 1100, 1050, 1200}, 1000,
                SubMsGrowth.GrowthClass.AMPLIFICATION_BOUNDED, 3.0);
        var report = SubMsGrowth.grow(r, "java");
        assertTrue(report.verdict().holds(), report.verdict().summary());
        assertEquals(1.2, report.verdict().observed(), 1e-9);
        assertNull(SubMsGrowth.assertGrowthHolds(report));
    }

    @Test
    void amplificationBoundedBreachesWhenDiskGrowsButLiveFlat() {
        var r = new ScriptedRecipe(new long[] {1000, 5000, 12000, 21000}, 1000,
                SubMsGrowth.GrowthClass.AMPLIFICATION_BOUNDED, 3.0);
        var report = SubMsGrowth.grow(r, "java");
        assertFalse(report.verdict().holds());
        assertEquals(21.0, report.verdict().observed(), 1e-9);
        assertNotNull(SubMsGrowth.assertGrowthHolds(report));
    }

    @Test
    void plateauHoldsWhenFlatAndBreachesWhenClimbing() {
        var flat = new ScriptedRecipe(new long[] {66_000, 66_000, 66_000, 66_000}, 20,
                SubMsGrowth.GrowthClass.PLATEAU_BOUNDED, 1.5);
        var held = SubMsGrowth.grow(flat, "java");
        assertTrue(held.verdict().holds(), held.verdict().summary());

        // Baseline is the mid-point (round 3 here); last / mid = 16000 / 4000.
        var climbing = new ScriptedRecipe(new long[] {1000, 2000, 4000, 16000}, 20,
                SubMsGrowth.GrowthClass.PLATEAU_BOUNDED, 1.5);
        var leak = SubMsGrowth.grow(climbing, "java");
        assertFalse(leak.verdict().holds());
        assertEquals(4.0, leak.verdict().observed(), 1e-9);
    }

    @Test
    void boundedGatesOnPeakBytes() {
        var r = new ScriptedRecipe(new long[] {100, 128, 128, 128}, 128,
                SubMsGrowth.GrowthClass.BOUNDED, 128.0);
        assertTrue(SubMsGrowth.grow(r, "java").verdict().holds());
    }

    @Test
    void unboundedAlwaysHolds() {
        var r = new ScriptedRecipe(new long[] {1, 10, 100, 100_000}, 1,
                SubMsGrowth.GrowthClass.UNBOUNDED_OK, 0.0);
        var report = SubMsGrowth.grow(r, "java");
        assertTrue(report.verdict().holds());
        assertEquals("growth expected and unbounded by design", report.verdict().summary());
    }

    @Test
    void amplificationIsZeroWhenNothingIsLive() {
        var r = new ScriptedRecipe(new long[] {512, 512}, 0,
                SubMsGrowth.GrowthClass.AMPLIFICATION_BOUNDED, 3.0);
        var report = SubMsGrowth.grow(r, "java");
        // Every round is filtered out of the max, so the observed ratio is 0 and
        // an all-empty run cannot breach.
        assertEquals(0.0, report.rounds().get(0).amplification(), 1e-9);
        assertTrue(report.verdict().holds());
    }

    @Test
    void totalBytesIsDiskPlusMemory() {
        var r = new SubMsGrowthRecipe() {
            @Override public String name() {
                return "mixed";
            }

            @Override public int rounds() {
                return 1;
            }

            @Override public int opsPerRound() {
                return 2;
            }

            @Override public void op(int round, int i) {}

            @Override public long diskBytes() {
                return 300;
            }

            @Override public long memoryBytes() {
                return 700;
            }

            @Override public long liveBytes() {
                return 500;
            }

            @Override public SubMsGrowth.GrowthClass expectedClass() {
                return SubMsGrowth.GrowthClass.AMPLIFICATION_BOUNDED;
            }

            @Override public double expectedBound() {
                return 3.0;
            }
        };
        var round = SubMsGrowth.grow(r, "java").rounds().get(0);
        assertEquals(1000, round.totalBytes());
        assertEquals(2.0, round.amplification(), 1e-9);
    }

    @Test
    void defaultsAreOpNameAndNoStructures() {
        var r = new ScriptedRecipe(new long[] {1}, 1, SubMsGrowth.GrowthClass.UNBOUNDED_OK, 0.0);
        var report = SubMsGrowth.grow(r, "java");
        assertEquals("op", report.opName());
        assertFalse(report.compact());
        assertEquals(0, report.rounds().get(0).memoryBytes());
    }

    @Test
    void roundsAndOpsAreFlooredAtOne() {
        var r = new SubMsGrowthRecipe() {
            @Override public String name() {
                return "degenerate";
            }

            @Override public int rounds() {
                return 0;
            }

            @Override public int opsPerRound() {
                return -5;
            }

            @Override public void op(int round, int i) {}

            @Override public long liveBytes() {
                return 1;
            }

            @Override public SubMsGrowth.GrowthClass expectedClass() {
                return SubMsGrowth.GrowthClass.UNBOUNDED_OK;
            }

            @Override public double expectedBound() {
                return 0.0;
            }
        };
        var report = SubMsGrowth.grow(r, "java");
        assertEquals(1, report.rounds().size());
        assertEquals(1, report.rounds().get(0).ops());
    }

    @Test
    void cumulativeOpsAccumulate() {
        var r = new ScriptedRecipe(new long[] {1, 2, 3}, 1,
                SubMsGrowth.GrowthClass.UNBOUNDED_OK, 0.0);
        var rounds = SubMsGrowth.grow(r, "java").rounds();
        assertEquals(4, rounds.get(0).cumulativeOps());
        assertEquals(8, rounds.get(1).cumulativeOps());
        assertEquals(12, rounds.get(2).cumulativeOps());
    }

    @Test
    void classWireTokensAreStable() {
        assertEquals("bounded", SubMsGrowth.GrowthClass.BOUNDED.asString());
        assertEquals("amplification_bounded",
                SubMsGrowth.GrowthClass.AMPLIFICATION_BOUNDED.asString());
        assertEquals("plateau_bounded", SubMsGrowth.GrowthClass.PLATEAU_BOUNDED.asString());
        assertEquals("unbounded_ok", SubMsGrowth.GrowthClass.UNBOUNDED_OK.asString());
    }

    @Test
    void jsonHasVerdictAndRounds() throws Exception {
        var r = new ScriptedRecipe(new long[] {1000, 1100}, 1000,
                SubMsGrowth.GrowthClass.AMPLIFICATION_BOUNDED, 3.0);
        var out = new StringWriter();
        SubMsGrowth.growthToJson(SubMsGrowth.grow(r, "java"), out);
        String s = out.toString();
        assertTrue(s.contains("\"kind\":\"growth\""));
        assertTrue(s.contains("\"class\":\"amplification_bounded\""));
        assertTrue(s.contains("\"holds\":true"));
        assertTrue(s.contains("\"sstables\":2"));
        assertTrue(s.contains("\"amplification\":"));
    }

    @Test
    void jsonEscapesStringsAndEmitsMeta() throws Exception {
        var r = new SubMsGrowthRecipe() {
            @Override public String name() {
                return "a\"b\\c\nd\re\tf\u0001";
            }

            @Override public int rounds() {
                return 1;
            }

            @Override public int opsPerRound() {
                return 1;
            }

            @Override public void op(int round, int i) {}

            @Override public long liveBytes() {
                return 1;
            }

            @Override public SubMsGrowth.GrowthClass expectedClass() {
                return SubMsGrowth.GrowthClass.UNBOUNDED_OK;
            }

            @Override public double expectedBound() {
                return 0.0;
            }
        };
        var base = SubMsGrowth.grow(r, "java");
        var withMeta = new SubMsGrowth.Report(base.workload(), base.lang(), base.opName(),
                base.rounds(), base.verdict(), base.compact(), Map.of("host", "ci-1"));
        var out = new StringWriter();
        SubMsGrowth.growthToJson(withMeta, out);
        String s = out.toString();
        assertTrue(s.contains("\"workload\":\"a\\\"b\\\\c\\nd\\re\\tf\\u0001\""), s);
        assertTrue(s.contains(",\"meta\":{\"host\":\"ci-1\"}"), s);
    }

    /**
     * The exact bytes of a growth document, latencies zeroed so the only variable
     * left is the encoder. The Rust suite pins this same string
     * (growth_tests.rs::json_bytes_match_the_cross_port_fixture).
     *
     * <p>Round 2 is 1568/1024 = 1.53125 exactly - a tie at the 5th decimal - so
     * this also pins the rounding at the cut. Rust's {@code {:.4}} is
     * half-to-even and writes 1.5312; {@code String.format("%.4f", ..)} is
     * half-up and would write 1.5313. That one digit is the whole reason the
     * encoder goes through BigDecimal.
     */
    @Test
    void jsonBytesMatchTheCrossPortFixture() throws Exception {
        var r = new ScriptedRecipe(new long[] {1000, 1568}, 1024,
                SubMsGrowth.GrowthClass.AMPLIFICATION_BOUNDED, 3.0);
        var base = SubMsGrowth.grow(r, "java");

        List<SubMsGrowth.Round> zeroed = new ArrayList<>();
        for (var x : base.rounds()) {
            zeroed.add(new SubMsGrowth.Round(x.round(), x.ops(), x.cumulativeOps(), x.diskBytes(),
                    x.memoryBytes(), x.totalBytes(), x.liveBytes(), x.amplification(),
                    x.structures(), 0, 0, 0));
        }
        var fixture = new SubMsGrowth.Report(base.workload(), "fixture", base.opName(), zeroed,
                base.verdict(), base.compact(), Map.of());

        var out = new StringWriter();
        SubMsGrowth.growthToJson(fixture, out);
        assertEquals(GROWTH_JSON_FIXTURE, out.toString());
    }

    private static final String GROWTH_JSON_FIXTURE =
            "{\"kind\":\"growth\",\"workload\":\"scripted\",\"lang\":\"fixture\",\"op\":\"op\","
            + "\"growth_version\":2,"
            + "\"verdict\":{\"class\":\"amplification_bounded\",\"bound\":3.0000,\"holds\":true,"
            + "\"observed\":1.5312,"
            + "\"summary\":\"max footprint/live amplification 1.53x vs ceiling 3.00x\"},"
            + "\"compact\":false,\"rounds\":["
            + "{\"round\":1,\"ops\":4,\"cumulative_ops\":4,\"disk_bytes\":1000,\"memory_bytes\":0,"
            + "\"total_bytes\":1000,\"live_bytes\":1024,\"amplification\":0.9766,"
            + "\"structures\":{\"sstables\":1},\"p50_ns\":0,\"p99_ns\":0,\"max_ns\":0},"
            + "{\"round\":2,\"ops\":4,\"cumulative_ops\":8,\"disk_bytes\":1568,\"memory_bytes\":0,"
            + "\"total_bytes\":1568,\"live_bytes\":1024,\"amplification\":1.5312,"
            + "\"structures\":{\"sstables\":2},\"p50_ns\":0,\"p99_ns\":0,\"max_ns\":0}"
            + "]}";
}
