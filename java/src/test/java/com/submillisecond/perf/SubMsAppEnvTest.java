package com.submillisecond.perf;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

class SubMsAppEnvTest {

    @Test
    void parses_canonical_lowercase() {
        assertEquals(SubMsAppEnv.LOCAL, SubMsAppEnv.parse("local"));
        assertEquals(SubMsAppEnv.DEV,   SubMsAppEnv.parse("dev"));
        assertEquals(SubMsAppEnv.UAT,   SubMsAppEnv.parse("uat"));
        assertEquals(SubMsAppEnv.PROD,  SubMsAppEnv.parse("prod"));
    }

    @Test
    void parses_mixed_case() {
        assertEquals(SubMsAppEnv.LOCAL, SubMsAppEnv.parse("LOCAL"));
        assertEquals(SubMsAppEnv.DEV,   SubMsAppEnv.parse("Dev"));
        assertEquals(SubMsAppEnv.PROD,  SubMsAppEnv.parse("PROD"));
        assertEquals(SubMsAppEnv.UAT,   SubMsAppEnv.parse("uAt"));
    }

    @Test
    void parses_synonyms() {
        assertEquals(SubMsAppEnv.DEV,  SubMsAppEnv.parse("development"));
        assertEquals(SubMsAppEnv.PROD, SubMsAppEnv.parse("production"));
        assertEquals(SubMsAppEnv.UAT,  SubMsAppEnv.parse("staging"));
        assertEquals(SubMsAppEnv.UAT,  SubMsAppEnv.parse("stage"));
    }

    @Test
    void trims_whitespace() {
        assertEquals(SubMsAppEnv.PROD, SubMsAppEnv.parse("  prod  "));
        assertEquals(SubMsAppEnv.DEV,  SubMsAppEnv.parse("\tdev\n"));
    }

    @Test
    void unknown_falls_back_to_local() {
        assertEquals(SubMsAppEnv.LOCAL, SubMsAppEnv.parse(""));
        assertEquals(SubMsAppEnv.LOCAL, SubMsAppEnv.parse("nonsense"));
        assertEquals(SubMsAppEnv.LOCAL, SubMsAppEnv.parse("preprod"));
    }

    @Test
    void null_falls_back_to_local() {
        assertEquals(SubMsAppEnv.LOCAL, SubMsAppEnv.parse(null));
    }

    @Test
    void as_string_is_lowercase() {
        assertEquals("local", SubMsAppEnv.LOCAL.asString());
        assertEquals("dev",   SubMsAppEnv.DEV.asString());
        assertEquals("uat",   SubMsAppEnv.UAT.asString());
        assertEquals("prod",  SubMsAppEnv.PROD.asString());
    }

    @Test
    void to_string_matches_as_string() {
        assertEquals("prod", SubMsAppEnv.PROD.toString());
        assertEquals("local", SubMsAppEnv.LOCAL.toString());
    }

    @Test
    void from_env_matches_parse_on_current_env() {
        // Property-style: whatever APP_ENV the test JVM has (or doesn't),
        // fromEnv() should be parse(System.getenv("APP_ENV")).
        assertEquals(SubMsAppEnv.parse(System.getenv("APP_ENV")), SubMsAppEnv.fromEnv());
    }

    @Test
    void enum_values_are_stable() {
        // Lock the ordering so downstream consumers that switch on ordinal
        // aren't surprised by reorderings.
        SubMsAppEnv[] vs = SubMsAppEnv.values();
        assertEquals(SubMsAppEnv.LOCAL, vs[0]);
        assertEquals(SubMsAppEnv.DEV,   vs[1]);
        assertEquals(SubMsAppEnv.UAT,   vs[2]);
        assertEquals(SubMsAppEnv.PROD,  vs[3]);
    }
}
