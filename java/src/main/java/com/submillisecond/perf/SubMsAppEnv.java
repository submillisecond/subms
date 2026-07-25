package com.submillisecond.perf;

/**
 * Deployment environment. {@link #LOCAL} is the safe default when
 * {@code APP_ENV} is unset, empty, or holds an unrecognised value.
 */
public enum SubMsAppEnv {
    LOCAL,
    DEV,
    UAT,
    PROD;

    /** Read {@code APP_ENV} from the process environment. */
    public static SubMsAppEnv fromEnv() {
        return parse(System.getenv("APP_ENV"));
    }

    /** Map a string (case-insensitive, trimmed) to an enum value. */
    public static SubMsAppEnv parse(String raw) {
        if (raw == null) return LOCAL;
        switch (raw.trim().toLowerCase(java.util.Locale.ROOT)) {
            case "local":
                return LOCAL;
            case "dev":
            case "development":
                return DEV;
            case "uat":
            case "staging":
            case "stage":
                return UAT;
            case "prod":
            case "production":
                return PROD;
            default:
                return LOCAL;
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
