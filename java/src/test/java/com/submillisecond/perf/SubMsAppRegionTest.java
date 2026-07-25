package com.submillisecond.perf;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

class SubMsAppRegionTest {

    @Test
    void parses_canonical() {
        assertEquals(SubMsAppRegion.NA,    SubMsAppRegion.parse("na"));
        assertEquals(SubMsAppRegion.LATAM, SubMsAppRegion.parse("latam"));
        assertEquals(SubMsAppRegion.EMEA,  SubMsAppRegion.parse("emea"));
        assertEquals(SubMsAppRegion.APAC,  SubMsAppRegion.parse("apac"));
    }

    @Test
    void parses_mixed_case() {
        assertEquals(SubMsAppRegion.NA,   SubMsAppRegion.parse("NA"));
        assertEquals(SubMsAppRegion.EMEA, SubMsAppRegion.parse("EMEA"));
        assertEquals(SubMsAppRegion.APAC, SubMsAppRegion.parse("ApAc"));
    }

    @Test
    void parses_synonyms() {
        assertEquals(SubMsAppRegion.NA,    SubMsAppRegion.parse("north-america"));
        assertEquals(SubMsAppRegion.NA,    SubMsAppRegion.parse("north_america"));
        assertEquals(SubMsAppRegion.NA,    SubMsAppRegion.parse("namer"));
        assertEquals(SubMsAppRegion.LATAM, SubMsAppRegion.parse("lat-am"));
        assertEquals(SubMsAppRegion.LATAM, SubMsAppRegion.parse("latin-america"));
        assertEquals(SubMsAppRegion.APAC,  SubMsAppRegion.parse("asia-pacific"));
    }

    @Test
    void trims_whitespace() {
        assertEquals(SubMsAppRegion.EMEA, SubMsAppRegion.parse("  emea  "));
        assertEquals(SubMsAppRegion.NA,   SubMsAppRegion.parse("\tna\n"));
    }

    @Test
    void unknown_falls_back_to_unknown() {
        assertEquals(SubMsAppRegion.UNKNOWN, SubMsAppRegion.parse(""));
        assertEquals(SubMsAppRegion.UNKNOWN, SubMsAppRegion.parse("moon"));
        assertEquals(SubMsAppRegion.UNKNOWN, SubMsAppRegion.parse("antarctica"));
    }

    @Test
    void null_falls_back_to_unknown() {
        assertEquals(SubMsAppRegion.UNKNOWN, SubMsAppRegion.parse(null));
    }

    @Test
    void as_string_is_lowercase() {
        assertEquals("na",      SubMsAppRegion.NA.asString());
        assertEquals("latam",   SubMsAppRegion.LATAM.asString());
        assertEquals("emea",    SubMsAppRegion.EMEA.asString());
        assertEquals("apac",    SubMsAppRegion.APAC.asString());
        assertEquals("unknown", SubMsAppRegion.UNKNOWN.asString());
    }

    @Test
    void from_env_matches_parse_on_current_env() {
        assertEquals(SubMsAppRegion.parse(System.getenv("APP_REGION")), SubMsAppRegion.fromEnv());
    }

    @Test
    void enum_values_are_stable() {
        SubMsAppRegion[] vs = SubMsAppRegion.values();
        assertEquals(SubMsAppRegion.NA,      vs[0]);
        assertEquals(SubMsAppRegion.LATAM,   vs[1]);
        assertEquals(SubMsAppRegion.EMEA,    vs[2]);
        assertEquals(SubMsAppRegion.APAC,    vs[3]);
        assertEquals(SubMsAppRegion.UNKNOWN, vs[4]);
    }
}
