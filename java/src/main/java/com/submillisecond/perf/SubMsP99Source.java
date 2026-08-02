package com.submillisecond.perf;

/**
 * Which box a manifest's {@code p99ByStage} figures were measured on.
 *
 * <p>The feature bench runs wherever it is invoked, so a manifest that carries
 * numbers without saying where they came from is indistinguishable from one
 * captured on the conformance box. Consumers treat an unstamped manifest as
 * {@link #LOCAL} and do not publish its numbers.
 *
 * <p>Mirrors {@code SubMsP99Source} in the Rust port; the wire tokens are
 * identical, so a manifest written by either side reads on both.
 */
public enum SubMsP99Source {
    /**
     * A dev machine. The CATEGORY still holds - it is read from the shape of a
     * size sweep, which does not depend on the box - but the numbers do not.
     */
    LOCAL("local"),
    /** The conformance fleet box, identified by its EC2 instance id. */
    FLEET("fleet");

    private final String wire;

    SubMsP99Source(String wire) {
        this.wire = wire;
    }

    /** The lowercase wire token written to {@code .subms/features/<lang>.json}. */
    public String asString() {
        return wire;
    }

    /**
     * Parse a wire token. Anything unrecognised reads as {@link #LOCAL}, so a typo
     * withholds numbers instead of publishing them.
     */
    public static SubMsP99Source fromWire(String s) {
        return FLEET.wire.equals(s) ? FLEET : LOCAL;
    }

    /** The instance id a fleet run was stamped with, or null off the fleet. */
    public static final String ENV_INSTANCE = "SUBMS_FLEET_INSTANCE";

    /**
     * Read provenance from the environment: FLEET plus the instance id when
     * {@code SUBMS_FLEET_INSTANCE} names a box, LOCAL otherwise.
     *
     * <p>The env var is the contract between the fleet orchestrator and every
     * recipe's {@code PerfFeaturesMain}, so no recipe hand-rolls its own
     * detection - and a run anywhere else is LOCAL by omission rather than by
     * remembering to say so.
     *
     * @return the instance id, or null when this is not a fleet run
     */
    public static String instanceFromEnv() {
        String id = System.getenv(ENV_INSTANCE);
        return id == null || id.trim().isEmpty() ? null : id.trim();
    }

    /** The source implied by the environment. */
    public static SubMsP99Source fromEnv() {
        return instanceFromEnv() == null ? LOCAL : FLEET;
    }
}
