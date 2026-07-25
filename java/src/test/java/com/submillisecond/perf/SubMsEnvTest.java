package com.submillisecond.perf;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

class SubMsEnvTest {

    // Use a UUID-suffixed key for the unset-key probes so we never collide
    // with a real env var. Static so a parallel test runner can't even
    // produce the same name twice.
    private static final String UNSET_KEY = "SUBMS_TEST_NEVER_SET_b9c2_38f1_4d10";

    @Test
    void env_str_returns_null_for_unset_key() {
        assertNull(SubMsEnv.envStr(UNSET_KEY));
    }

    @Test
    void env_or_falls_back_when_unset() {
        assertEquals("fallback", SubMsEnv.envOr(UNSET_KEY, "fallback"));
    }

    @Test
    void env_bool_falls_back_when_unset() {
        assertTrue(SubMsEnv.envBool(UNSET_KEY, true));
        assertFalse(SubMsEnv.envBool(UNSET_KEY, false));
    }

    @Test
    void env_int_falls_back_when_unset() {
        assertEquals(42, SubMsEnv.envInt(UNSET_KEY, 42));
    }

    @Test
    void env_long_falls_back_when_unset() {
        assertEquals(99L, SubMsEnv.envLong(UNSET_KEY, 99L));
    }

    @Test
    void env_double_falls_back_when_unset() {
        assertEquals(1.5, SubMsEnv.envDouble(UNSET_KEY, 1.5), 1e-9);
    }

    @Test
    void parse_bool_truthy_variants() {
        for (String v : new String[]{"true", "TRUE", "1", "yes", "on", " ON "}) {
            assertTrue(SubMsEnv.parseBool(v, false), "expected true for " + v);
        }
    }

    @Test
    void parse_bool_falsy_variants() {
        for (String v : new String[]{"false", "FALSE", "0", "no", "off"}) {
            assertFalse(SubMsEnv.parseBool(v, true), "expected false for " + v);
        }
    }

    @Test
    void parse_bool_falls_back_on_garbage() {
        assertTrue(SubMsEnv.parseBool("maybe", true));
        assertFalse(SubMsEnv.parseBool("maybe", false));
        assertTrue(SubMsEnv.parseBool(null, true));
    }

    @Test
    void parse_int_signed_and_trimmed() {
        assertEquals(-12345, SubMsEnv.parseInt(" -12345 ", 0));
        assertEquals(0,      SubMsEnv.parseInt("nope", 0));
        assertEquals(7,      SubMsEnv.parseInt(null, 7));
    }

    @Test
    void parse_long_handles_large_values() {
        assertEquals(9_000_000_000L,  SubMsEnv.parseLong("9000000000", -1L));
        assertEquals(-1L,             SubMsEnv.parseLong("oops", -1L));
    }

    @Test
    void parse_double_scientific_notation() {
        assertEquals(1.5e-3, SubMsEnv.parseDouble("1.5e-3", 0.0), 1e-12);
        assertEquals(0.0,    SubMsEnv.parseDouble("???",    0.0), 1e-12);
    }

    @Test
    void convenience_app_env_matches_direct_call() {
        assertEquals(SubMsAppEnv.fromEnv(), SubMsEnv.appEnv());
    }

    @Test
    void convenience_app_region_matches_direct_call() {
        assertEquals(SubMsAppRegion.fromEnv(), SubMsEnv.appRegion());
    }
}
