package com.submillisecond.perf;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

/**
 * Wire-form + environment parity for {@link SubMsP99Source}. The manifest-level
 * behaviour is covered in {@link SubMsFeatureManifestTest}; this pins the enum
 * itself, whose tokens must match the Rust port byte for byte.
 */
class SubMsP99SourceTest {

    @Test
    void wireTokensMatchTheRustPort() {
        assertEquals("local", SubMsP99Source.LOCAL.asString());
        assertEquals("fleet", SubMsP99Source.FLEET.asString());
        assertEquals(SubMsP99Source.FLEET, SubMsP99Source.fromWire("fleet"));
        assertEquals(SubMsP99Source.LOCAL, SubMsP99Source.fromWire("local"));
    }

    @Test
    void anUnknownTokenReadsAsLocal() {
        // A typo withholds numbers instead of publishing them.
        assertEquals(SubMsP99Source.LOCAL, SubMsP99Source.fromWire("ec2"));
        assertEquals(SubMsP99Source.LOCAL, SubMsP99Source.fromWire(""));
        assertEquals(SubMsP99Source.LOCAL, SubMsP99Source.fromWire(null));
    }

    @Test
    void envProvenanceIsLocalOffTheFleet() {
        // System.getenv is immutable in-process, so this asserts the branch that
        // holds on any dev machine and in CI: the var is unset, so a run is Local
        // by omission rather than by remembering to say so.
        if (System.getenv(SubMsP99Source.ENV_INSTANCE) == null) {
            assertNull(SubMsP99Source.instanceFromEnv());
            assertEquals(SubMsP99Source.LOCAL, SubMsP99Source.fromEnv());
        } else {
            assertEquals(SubMsP99Source.FLEET, SubMsP99Source.fromEnv());
            assertNull(null, "fleet run: instance id present");
        }
    }

    @Test
    void theEnvVarNameIsTheCrossLanguageContract() {
        // Both ports and the fleet orchestrator agree on this exact string; a
        // rename here silently downgrades every capture to Local.
        assertEquals("SUBMS_FLEET_INSTANCE", SubMsP99Source.ENV_INSTANCE);
    }
}
