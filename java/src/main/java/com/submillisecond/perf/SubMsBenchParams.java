package com.submillisecond.perf;

import java.io.IOException;
import java.util.Locale;
import java.util.Map;

/**
 * Workload params + stdin parsers. Mirrors the Rust `SubMsBenchParams` struct
 * and `params.rs` helpers.
 */
public record SubMsBenchParams(
        /** Work items per stage. */ int entries,
        /** Pre-timed warm-up iterations. */ int warmup,
        /** Deterministic RNG seed. */ long seed) {

    /** Standard defaults: 50k entries, 5k warm-up, seed 0. */
    public static SubMsBenchParams defaults() {
        return new SubMsBenchParams(50_000, 5_000, 0L);
    }

    /** Reads {@code entries}/{@code warmup}/{@code seed}; missing keys default. */
    public static SubMsBenchParams fromMap(Map<String, String> args) {
        SubMsBenchParams d = defaults();
        return new SubMsBenchParams(
                parseInt(args, "entries", d.entries),
                parseInt(args, "warmup", d.warmup),
                parseLong(args, "seed", d.seed));
    }

    /** Equivalent to {@code fromMap(SubMsPerfHarness.readStdinKv())}. */
    public static SubMsBenchParams fromStdin() throws IOException {
        return fromMap(SubMsPerfHarness.readStdinKv());
    }

    /** Parse an {@code int}; falls back to {@code defaultValue} if missing or unparseable. */
    public static int parseInt(Map<String, String> args, String key, int defaultValue) {
        String v = args.get(key);
        if (v == null) return defaultValue;
        try {
            return Integer.parseInt(v);
        } catch (NumberFormatException e) {
            return defaultValue;
        }
    }

    /** Parse a {@code long}; falls back to {@code defaultValue} if missing or unparseable. */
    public static long parseLong(Map<String, String> args, String key, long defaultValue) {
        String v = args.get(key);
        if (v == null) return defaultValue;
        try {
            return Long.parseLong(v);
        } catch (NumberFormatException e) {
            return defaultValue;
        }
    }

    /** Read a string; falls back to {@code defaultValue} if absent. */
    public static String parseString(Map<String, String> args, String key, String defaultValue) {
        String v = args.get(key);
        return v == null ? defaultValue : v;
    }

    /**
     * Accepts {@code 1}/{@code true}/{@code on}/{@code yes} (and the negations) case-insensitively.
     * Unknown or missing -> {@code defaultValue}.
     */
    public static boolean parseBool(Map<String, String> args, String key, boolean defaultValue) {
        String v = args.get(key);
        if (v == null) return defaultValue;
        return switch (v.toLowerCase(Locale.ROOT)) {
            case "1", "true", "on", "yes" -> true;
            case "0", "false", "off", "no" -> false;
            default -> defaultValue;
        };
    }
}
