package com.submillisecond.perf;

import java.util.Locale;

/**
 * Tiny env-var utility surface. Mirrors the Rust {@code subms::env::*}
 * functions byte-for-byte at the behavioural level. Unset and empty env
 * vars are treated identically (both return absent / default).
 */
public final class SubMsEnv {
    private SubMsEnv() {}

    /** Returns null for unset OR empty. */
    public static String envStr(String key) {
        String v = System.getenv(key);
        return (v == null || v.isEmpty()) ? null : v;
    }

    public static String envOr(String key, String def) {
        String v = envStr(key);
        return v != null ? v : def;
    }

    public static boolean envBool(String key, boolean def) {
        return parseBool(envStr(key), def);
    }

    public static int envInt(String key, int def) {
        return parseInt(envStr(key), def);
    }

    public static long envLong(String key, long def) {
        return parseLong(envStr(key), def);
    }

    public static double envDouble(String key, double def) {
        return parseDouble(envStr(key), def);
    }

    /** Parse a string the way {@link #envBool} would parse the same env value. */
    public static boolean parseBool(String raw, boolean def) {
        if (raw == null) return def;
        switch (raw.trim().toLowerCase(Locale.ROOT)) {
            case "true":
            case "1":
            case "yes":
            case "on":
                return true;
            case "false":
            case "0":
            case "no":
            case "off":
                return false;
            default:
                return def;
        }
    }

    public static int parseInt(String raw, int def) {
        if (raw == null) return def;
        try {
            return Integer.parseInt(raw.trim());
        } catch (NumberFormatException ignored) {
            return def;
        }
    }

    public static long parseLong(String raw, long def) {
        if (raw == null) return def;
        try {
            return Long.parseLong(raw.trim());
        } catch (NumberFormatException ignored) {
            return def;
        }
    }

    public static double parseDouble(String raw, double def) {
        if (raw == null) return def;
        try {
            return Double.parseDouble(raw.trim());
        } catch (NumberFormatException ignored) {
            return def;
        }
    }

    /** Convenience: read {@code APP_ENV} via {@link SubMsAppEnv#fromEnv()}. */
    public static SubMsAppEnv appEnv() {
        return SubMsAppEnv.fromEnv();
    }

    /** Convenience: read {@code APP_REGION} via {@link SubMsAppRegion#fromEnv()}. */
    public static SubMsAppRegion appRegion() {
        return SubMsAppRegion.fromEnv();
    }
}
