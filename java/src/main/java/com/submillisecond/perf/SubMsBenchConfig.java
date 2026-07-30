package com.submillisecond.perf;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.OptionalLong;

/**
 * Per-recipe bench configuration - the typed view of a recipe's
 * {@code .subms/perf/controls.json}. The Java sibling of the Rust
 * {@code SubMsBenchConfig}.
 *
 * <p>Like {@link SubMsFeatureManifest} it loads, merge-updates, and saves through
 * the shared zero-dependency JSON value model that PRESERVES every field this
 * harness does not own (the fleet orchestrator's {@code sample_cap}/{@code rounds},
 * a third party's custom keys) across a round-trip - a setter touches only the key
 * it names.
 *
 * <p>One semantic default worth stating: an ABSENT {@code cpu_pin} means
 * {@link SubMsCpuPin#SINGLE}. The historical behaviour is to pin a single-threaded
 * recipe to one isolated core for a stable p99; a multi-threaded recipe (its own
 * writer plus a worker thread) sets {@code "multi"} (with {@code cores}) or
 * {@code "none"} so a single-core pin does not starve it.
 */
public final class SubMsBenchConfig {

    private final LinkedHashMap<String, Object> root;

    /** An empty config (every accessor returns its documented default). */
    public SubMsBenchConfig() {
        this.root = new LinkedHashMap<>();
    }

    private SubMsBenchConfig(LinkedHashMap<String, Object> root) {
        this.root = root;
    }

    /**
     * Parse from JSON text. A missing, empty, malformed, or non-object input yields
     * an empty config rather than throwing - the accessors fall back to defaults, so
     * a broken controls.json never breaks a bench.
     */
    @SuppressWarnings("unchecked")
    public static SubMsBenchConfig loadStr(String text) {
        if (text == null || text.trim().isEmpty()) {
            return new SubMsBenchConfig();
        }
        Object parsed;
        try {
            parsed = SubMsFeatureManifest.parseJson(text);
        } catch (RuntimeException e) {
            return new SubMsBenchConfig();
        }
        if (!(parsed instanceof LinkedHashMap)) {
            return new SubMsBenchConfig();
        }
        return new SubMsBenchConfig((LinkedHashMap<String, Object>) parsed);
    }

    /** Load from a file; a missing file is not an error (returns an empty config). */
    public static SubMsBenchConfig load(Path path) throws IOException {
        if (!Files.exists(path)) {
            return new SubMsBenchConfig();
        }
        return loadStr(Files.readString(path));
    }

    /** Serialise to pretty JSON (stable insertion-order keys). */
    public String toJson() {
        return SubMsFeatureManifest.jsonToString(root);
    }

    /** Write to {@code path}, creating parent directories if needed. */
    public void save(Path path) throws IOException {
        if (path.getParent() != null) {
            Files.createDirectories(path.getParent());
        }
        Files.writeString(path, toJson());
    }

    // ---- typed accessors (absent -> documented default) ----

    /**
     * How the bench harness should place this recipe on the box's CPUs. ABSENT (or
     * unrecognised) defaults to {@link SubMsCpuPin#SINGLE} - a single-threaded recipe
     * wants one isolated core for a stable p99. Tolerates the legacy boolean form
     * ({@code true} -> SINGLE, {@code false} -> NONE).
     */
    public SubMsCpuPin cpuPin() {
        Object v = root.get("cpu_pin");
        if (v instanceof String) {
            SubMsCpuPin p = SubMsCpuPin.fromWire((String) v);
            return p != null ? p : SubMsCpuPin.SINGLE;
        }
        if (v instanceof Boolean) {
            return ((Boolean) v) ? SubMsCpuPin.SINGLE : SubMsCpuPin.NONE;
        }
        return SubMsCpuPin.SINGLE;
    }

    /** How many cores a {@link SubMsCpuPin#MULTI} recipe wants pinned; empty = unset. */
    public OptionalLong cores() {
        return num("cores");
    }

    /** The per-op sample cap the capture should raise the harness to; empty = default. */
    public OptionalLong sampleCap() {
        return num("sample_cap");
    }

    /** Free-text note on why the config is set the way it is; {@code null} if absent. */
    public String reason() {
        Object v = root.get("reason");
        return (v instanceof String) ? (String) v : null;
    }

    // ---- setters (merge-preserving; touch only the named key) ----

    public SubMsBenchConfig setCpuPin(SubMsCpuPin mode) {
        root.put("cpu_pin", mode.wire());
        return this;
    }

    public SubMsBenchConfig setCores(long cores) {
        root.put("cores", new SubMsFeatureManifest.Num(Long.toString(cores)));
        return this;
    }

    public SubMsBenchConfig setSampleCap(long cap) {
        root.put("sample_cap", new SubMsFeatureManifest.Num(Long.toString(cap)));
        return this;
    }

    public SubMsBenchConfig setReason(String reason) {
        root.put("reason", reason);
        return this;
    }

    // ---- internals ----

    private OptionalLong num(String key) {
        Object v = root.get(key);
        String raw = null;
        if (v instanceof SubMsFeatureManifest.Num) {
            raw = ((SubMsFeatureManifest.Num) v).raw;
        } else if (v instanceof Long) {
            raw = Long.toString((Long) v);
        } else if (v instanceof Integer) {
            raw = Integer.toString((Integer) v);
        }
        if (raw == null) {
            return OptionalLong.empty();
        }
        try {
            return OptionalLong.of(Long.parseLong(raw.trim()));
        } catch (NumberFormatException e) {
            return OptionalLong.empty();
        }
    }
}
