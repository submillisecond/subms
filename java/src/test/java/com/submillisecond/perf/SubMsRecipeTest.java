package com.submillisecond.perf;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SubMsRecipeTest {

    /** Inline recipe that exercises both stages and harness inputs/metadata. */
    static final class TinyRecipe implements SubMsRecipe {
        @Override public String name() { return "tiny"; }
        @Override public void run(SubMsPerfHarness h, SubMsBenchParams params) {
            h.meta("tag", "smoke");
            SubMsPerfHarness.Stage warmup = h.stage("warmup", params.warmup());
            for (int i = 0; i < params.warmup(); i++) warmup.record(i);
            SubMsPerfHarness.Stage work = h.stage("work", params.entries());
            for (int i = 0; i < params.entries(); i++) work.record(i * 2L);
        }
    }

    @Test
    void recipeRunsThroughHarness() {
        SubMsBenchParams p = new SubMsBenchParams(10, 3, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new TinyRecipe(), p);
        assertEquals(3, h.stage("warmup").count());
        assertEquals(10, h.stage("work").count());
    }

    @Test
    void recipeNameSurfacesAsWorkload() throws Exception {
        SubMsPerfHarness h = SubMsBench.runBench(new TinyRecipe(), new SubMsBenchParams(2, 1, 0L));
        java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
        try (java.io.PrintStream ps = new java.io.PrintStream(out, true, "UTF-8")) {
            h.writeJson(ps);
        }
        String json = out.toString("UTF-8");
        assertTrue(json.contains("\"workload\":\"tiny\""), json);
        assertTrue(json.contains("\"tag\":\"smoke\""), json);
    }
}
