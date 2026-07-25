package com.submillisecond.perf;

/**
 * Deployment region. {@link #UNKNOWN} is the safe default when
 * {@code APP_REGION} is unset, empty, or holds an unrecognised value.
 */
public enum SubMsAppRegion {
    NA,
    LATAM,
    EMEA,
    APAC,
    UNKNOWN;

    /** Read {@code APP_REGION} from the process environment. */
    public static SubMsAppRegion fromEnv() {
        return parse(System.getenv("APP_REGION"));
    }

    /** Map a string (case-insensitive, trimmed) to an enum value. */
    public static SubMsAppRegion parse(String raw) {
        if (raw == null) return UNKNOWN;
        switch (raw.trim().toLowerCase(java.util.Locale.ROOT)) {
            case "na":
            case "north-america":
            case "north_america":
            case "namer":
                return NA;
            case "latam":
            case "lat-am":
            case "latin-america":
            case "latin_america":
                return LATAM;
            case "emea":
                return EMEA;
            case "apac":
            case "asia-pacific":
            case "asia_pacific":
                return APAC;
            default:
                return UNKNOWN;
        }
    }

    /** Lowercase canonical name; mirror of the Rust {@code as_str()}. */
    public String asString() {
        return name().toLowerCase(java.util.Locale.ROOT);
    }

    @Override
    public String toString() {
        return asString();
    }
}
