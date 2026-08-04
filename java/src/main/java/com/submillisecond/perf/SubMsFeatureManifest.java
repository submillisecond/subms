package com.submillisecond.perf;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

/**
 * Per-feature latency classification + manifest. Mirrors Rust's
 * {@code subms::feature}.
 *
 * <p>A library's optional features are not all the same kind of thing: some
 * change the per-op hot path (a measured p99 claim), some are O(n) whole-structure
 * ops (serialize, compaction) that cannot honestly carry a per-op sub-ms number,
 * and some are pure capabilities with no latency delta (a serde/JSON derive). This
 * class lets the <em>bench decide</em> the category from a size sweep
 * ({@link #classify}), and merge-writes the decision into a per-language manifest
 * {@code .subms/features/<lang>.json} that the website renders.
 *
 * <p>The manifest is a public contract: the merge preserves every field the
 * harness does not own (yours, or another tool's), touching only a feature's
 * {@code perf} rating + {@code p99ByStage}. Zero-dependency: the JSON value model,
 * parser and serialiser here are hand-written (JDK only), like the rest of the lib.
 */
public final class SubMsFeatureManifest {

    /**
     * A feature that grows its p99 by at least this fraction of the relative size
     * growth is O(n)-ish -&gt; structural. 0.5 keeps O(log n) / O(1) firmly on the
     * hot path while catching genuine linear scaling.
     */
    private static final double STRUCTURAL_FRACTION = 0.5;
    /**
     * A flat feature whose p99 is within this fraction of the base op is a measured
     * non-effect -&gt; auxiliary.
     */
    private static final double HOT_PATH_DELTA = 0.10;
    /**
     * Per-op figures above this are REPORTED, never claimed. The site's promise is
     * p99 &lt; 1 ms, so a flat op costing more has no business carrying a per-op
     * claim however size-independent it is.
     */
    private static final long CLAIM_LINE_NS = 1_000_000L;
    /**
     * Scaling test: how close to the structural guard counts as "too close to
     * call", as a fraction of the growth the guard demands.
     */
    private static final double INDETERMINATE_BAND = 0.25;
    /**
     * Base-delta test: half-width of the undecidable band, in EXCESS over base -
     * indeterminate at 5%..15% above base.
     *
     * <p>Deliberately on the EXCESS rather than the guard. The guard sits only 10%
     * above base, so a band around the guard wide enough to catch a straddler also
     * swallows a feature measuring exactly base - the least ambiguous auxiliary
     * there is.
     */
    private static final double BASE_DELTA_BAND = 0.05;

    /**
     * Provenance for features written in THIS run, stamped onto each by
     * {@link #setFeature}.
     *
     * <p>The file-level {@code p99_source} is a summary and cannot be trusted
     * alone: the manifest MERGE-writes, so a feature the current run did not
     * measure keeps its previous numbers while inheriting whatever the file says.
     * That is how a laptop-measured feature ended up inside a file stamped
     * {@code fleet}.
     */
    private SubMsP99Source runSource;

    private String runSourceRef;

    /** The outcome of a classification: the category and a short human reason. */
    public record Decision(SubMsFeatureCategory category, String reason) {}

    /**
     * One stage of a feature, classified on its own measurements.
     *
     * <p>A feature is not necessarily one kind of operation. {@code
     * concurrent-reads} on the adaptive radix tree has a 1.2 us {@code get} and a
     * 44 ms {@code snapshot}; a single category cannot describe both, and the
     * rolled-up one published the snapshot under the get's label.
     */
    public record StageClass(long p99Ns, SubMsFeatureCategory category, String reason) {}

    /**
     * How much a category restricts what the feature may claim. The rollup takes
     * the maximum, so one heavy stage cannot hide behind a fast one.
     */
    private static int restriction(SubMsFeatureCategory c) {
        switch (c) {
            case AUXILIARY:
                return 0;
            case HOT_PATH:
                return 1;
            case STRUCTURAL:
                return 2;
            case REPORTED:
                return 3;
            default:
                return 4;
        }
    }

    /**
     * Roll per-stage categories into the one a feature publishes.
     *
     * <p>The MOST RESTRICTIVE stage wins. A feature whose stages are
     * {@code hot-path} and {@code reported} is not a hot-path feature - it
     * contains an operation that cannot carry the claim, and summarising it as
     * hot-path is how a 44 ms snapshot came to sit under a per-op sub-ms label.
     * The per-stage detail stays in the manifest, so a conservative summary costs
     * the reader nothing.
     */
    public static SubMsFeatureCategory rollUpStages(Map<String, StageClass> stages) {
        SubMsFeatureCategory worst = SubMsFeatureCategory.AUXILIARY;
        if (stages == null) {
            return worst;
        }
        for (StageClass s : stages.values()) {
            if (restriction(s.category()) > restriction(worst)) {
                worst = s.category();
            }
        }
        return worst;
    }

    /**
     * Decide a feature's category from a size sweep of {@code {size, p99Ns}} rows.
     * {@code baseP99Ns} is the base op's p99 for the no-delta check (null -&gt; any
     * flat, non-empty sweep reads as hot-path). {@code override} short-circuits the
     * decision for a genuinely ambiguous feature; the reason records that.
     */
    public static Decision classify(long[][] sweep, Long baseP99Ns, SubMsFeatureCategory override) {
        if (override != null) {
            return new Decision(override, "override: pinned " + override.asString());
        }
        if (sweep == null || sweep.length == 0) {
            return new Decision(SubMsFeatureCategory.AUXILIARY, "no hot-path workload registered");
        }
        long minN = Long.MAX_VALUE, maxN = Long.MIN_VALUE;
        long p99AtMin = 0, p99AtMax = 0;
        for (long[] row : sweep) {
            if (row[0] < minN) {
                minN = row[0];
                p99AtMin = row[1];
            }
            if (row[0] > maxN) {
                maxN = row[0];
                p99AtMax = row[1];
            }
        }
        if (maxN > minN && p99AtMin > 0) {
            double sizeRatio = (double) maxN / (double) minN;
            double p99Ratio = (double) p99AtMax / (double) p99AtMin;
            double needed = STRUCTURAL_FRACTION * (sizeRatio - 1.0);
            double growth = p99Ratio - 1.0;
            if (needed > 0.0) {
                double margin = growth / needed;
                if (margin >= 1.0 - INDETERMINATE_BAND && margin <= 1.0 + INDETERMINATE_BAND) {
                    return new Decision(
                            SubMsFeatureCategory.INDETERMINATE,
                            String.format(
                                    "p99 grew %.1fx over %.0fx N, against %.1fx needed for structural - too close to call",
                                    p99Ratio, sizeRatio, needed + 1.0));
                }
            }
            if (growth >= needed) {
                return new Decision(
                        SubMsFeatureCategory.STRUCTURAL,
                        String.format(
                                "p99 scales with size (%.1fx over %.0fx N) - O(n)+, excluded from per-op claim",
                                p99Ratio, sizeRatio));
            }
        }
        long featureP99 = Math.max(p99AtMin, p99AtMax);
        // Above the claim line the hot-path/auxiliary question is moot: whatever
        // its delta against base, a figure this size cannot be a per-op sub-ms
        // claim. This bound is what stops a flat 30 ms op being published as one.
        if (featureP99 > CLAIM_LINE_NS) {
            return new Decision(
                    SubMsFeatureCategory.REPORTED,
                    "flat per-op p99 " + featureP99 + "ns, above the "
                            + (CLAIM_LINE_NS / 1_000_000L) + "ms claim line - reported, not claimed");
        }
        if (baseP99Ns != null && baseP99Ns > 0) {
            double excess = (double) featureP99 / (double) baseP99Ns - 1.0;
            if (excess >= HOT_PATH_DELTA - BASE_DELTA_BAND
                    && excess <= HOT_PATH_DELTA + BASE_DELTA_BAND) {
                return new Decision(
                        SubMsFeatureCategory.INDETERMINATE,
                        String.format(
                                "flat p99 %dns is %.0f%% over base %dns, against a %.0f%% hot-path guard - too close to call",
                                featureP99, excess * 100.0, baseP99Ns, HOT_PATH_DELTA * 100.0));
            }
            if ((double) featureP99 > (double) baseP99Ns * (1.0 + HOT_PATH_DELTA)) {
                return new Decision(
                        SubMsFeatureCategory.HOT_PATH,
                        "flat per-op p99 " + featureP99 + "ns, above base " + baseP99Ns + "ns");
            }
            if (featureP99 < baseP99Ns) {
                // Faster than base is still "no cost on the hot path", but it is
                // NOT "within 10% of base" - saying so of a figure a third of the
                // baseline makes the recorded reason wrong, and the reason is the
                // only audit trail the category has.
                return new Decision(
                        SubMsFeatureCategory.AUXILIARY,
                        "flat p99 " + featureP99 + "ns, at or below base " + baseP99Ns
                                + "ns - no hot-path cost");
            }
            return new Decision(
                    SubMsFeatureCategory.AUXILIARY,
                    String.format(
                            "flat p99 %dns within %.0f%% of base %dns - measured non-effect",
                            featureP99, HOT_PATH_DELTA * 100.0, baseP99Ns));
        }
        return new Decision(SubMsFeatureCategory.HOT_PATH, "flat per-op p99 " + featureP99 + "ns");
    }

    // ---------------- the manifest ----------------

    // JSON value model: LinkedHashMap<String,Object> (objects, order-preserving),
    // List<Object> (arrays), String, Num (a number literal), Boolean, or null.
    // Numbers keep their literal text so large integer p99 values round-trip exactly.
    private final LinkedHashMap<String, Object> root;

    private SubMsFeatureManifest(LinkedHashMap<String, Object> root) {
        this.root = root;
    }

    /** An empty manifest for {@code lang}. */
    public static SubMsFeatureManifest create(String lang) {
        LinkedHashMap<String, Object> r = new LinkedHashMap<>();
        r.put("lang", lang);
        r.put("features", new LinkedHashMap<String, Object>());
        return new SubMsFeatureManifest(r);
    }

    /** Parse an existing manifest, PRESERVING all fields; empty/invalid -&gt; new. */
    public static SubMsFeatureManifest loadStr(String lang, String text) {
        if (text == null || text.trim().isEmpty()) {
            return create(lang);
        }
        Object parsed;
        try {
            parsed = new JsonParser(text.trim()).parse();
        } catch (RuntimeException e) {
            return create(lang);
        }
        if (!(parsed instanceof LinkedHashMap)) {
            return create(lang);
        }
        @SuppressWarnings("unchecked")
        LinkedHashMap<String, Object> r = (LinkedHashMap<String, Object>) parsed;
        if (!(r.get("features") instanceof LinkedHashMap)) {
            r.put("features", new LinkedHashMap<String, Object>());
        }
        return new SubMsFeatureManifest(r);
    }

    /**
     * Stamp which box the {@code p99ByStage} figures in this manifest came from.
     *
     * <p>A latency number describes the machine that produced it, and the feature
     * bench runs wherever the author happens to be. Without this stamp a consumer
     * cannot tell a conformance-box capture from a laptop run, and the site's
     * renderer will not publish an unstamped number.
     *
     * <p>{@code reference} identifies the box when the source is
     * {@link SubMsP99Source#FLEET} - the EC2 instance id the capture ran on. It is
     * ignored for a local run, and any stale reference is cleared, so a manifest
     * cannot keep pointing at a fleet box after being re-run on a laptop.
     */
    public void setP99Source(SubMsP99Source source, String reference) {
        this.runSource = source;
        this.runSourceRef = reference;
        root.put("p99_source", source.asString());
        if (source == SubMsP99Source.FLEET && reference != null && !reference.isEmpty()) {
            root.put("p99_source_ref", reference);
        } else {
            root.remove("p99_source_ref");
        }
    }

    /**
     * The provenance currently recorded. An unstamped manifest reads as
     * {@link SubMsP99Source#LOCAL} - the conservative direction, since it
     * withholds numbers rather than publishing unattributed ones.
     */
    public SubMsP99Source p99Source() {
        Object v = root.get("p99_source");
        return v instanceof String ? SubMsP99Source.fromWire((String) v) : SubMsP99Source.LOCAL;
    }

    /** The EC2 instance id a fleet capture was stamped with, or null. */
    public String p99SourceRef() {
        Object v = root.get("p99_source_ref");
        return v instanceof String ? (String) v : null;
    }

    /**
     * Set (merge) a feature's rating + p99-by-stage, leaving every other field on
     * that feature, on other features, and at the top level untouched.
     */
    @SuppressWarnings("unchecked")
    public void setFeature(
            String name, SubMsFeatureCategory category, Map<String, Long> p99ByStage, String reason) {
        Object f = root.get("features");
        if (!(f instanceof LinkedHashMap)) {
            f = new LinkedHashMap<String, Object>();
            root.put("features", f);
        }
        LinkedHashMap<String, Object> features = (LinkedHashMap<String, Object>) f;
        Object e = features.get(name);
        if (!(e instanceof LinkedHashMap)) {
            e = new LinkedHashMap<String, Object>();
            features.put(name, e);
        }
        LinkedHashMap<String, Object> entry = (LinkedHashMap<String, Object>) e;
        entry.put("perf", category.asString());
        entry.put("perfReason", reason);
        if (p99ByStage == null || p99ByStage.isEmpty()) {
            entry.remove("p99ByStage");
        } else {
            LinkedHashMap<String, Object> stages = new LinkedHashMap<>();
            // TreeMap for a deterministic stage order in the output.
            for (Map.Entry<String, Long> s : new TreeMap<>(p99ByStage).entrySet()) {
                stages.put(s.getKey(), new Num(Long.toString(s.getValue())));
            }
            entry.put("p99ByStage", stages);
        }
        // Stamp the feature with the provenance of THIS run. The file-level field
        // says where the file was last touched; this says where these numbers came
        // from, which is the only one a reader can act on when the merge has left
        // older features in place beside fresh ones.
        if (runSource != null) {
            entry.put("p99Source", runSource.asString());
            if (runSource == SubMsP99Source.FLEET && runSourceRef != null && !runSourceRef.isEmpty()) {
                entry.put("p99SourceRef", runSourceRef);
            } else {
                entry.remove("p99SourceRef");
            }
        } else {
            // Nothing declared this run: drop a stale stamp rather than let a
            // previous run's provenance describe numbers it did not produce.
            entry.remove("p99Source");
            entry.remove("p99SourceRef");
        }
    }

    /**
     * Merge-write a feature classified PER STAGE.
     *
     * <p>Writes {@code stages} (each with its own p99, category and reason) and
     * sets the feature-level {@code perf} to the rollup, so a consumer reading
     * only the summary still cannot mistake a mixed feature for a uniformly hot
     * one. Preserves custom fields exactly like {@link #setFeature}.
     */
    @SuppressWarnings("unchecked")
    public void setFeatureStages(String name, Map<String, StageClass> stages, String reason) {
        SubMsFeatureCategory rolled = rollUpStages(stages);
        Map<String, Long> flat = new TreeMap<>();
        if (stages != null) {
            for (Map.Entry<String, StageClass> e : stages.entrySet()) {
                flat.put(e.getKey(), e.getValue().p99Ns());
            }
        }
        setFeature(name, rolled, flat, reason);
        Object f = root.get("features");
        if (!(f instanceof LinkedHashMap)) {
            return;
        }
        Object e = ((LinkedHashMap<String, Object>) f).get(name);
        if (!(e instanceof LinkedHashMap)) {
            return;
        }
        LinkedHashMap<String, Object> entry = (LinkedHashMap<String, Object>) e;
        if (stages == null || stages.isEmpty()) {
            entry.remove("stages");
            return;
        }
        LinkedHashMap<String, Object> rows = new LinkedHashMap<>();
        for (Map.Entry<String, StageClass> st : new TreeMap<>(stages).entrySet()) {
            LinkedHashMap<String, Object> row = new LinkedHashMap<>();
            row.put("p99", new Num(Long.toString(st.getValue().p99Ns())));
            row.put("perf", st.getValue().category().asString());
            row.put("perfReason", st.getValue().reason());
            rows.put(st.getKey(), row);
        }
        entry.put("stages", rows);
    }

    /**
     * The category recorded for one stage of a feature, if the manifest carries
     * per-stage detail for it.
     */
    @SuppressWarnings("unchecked")
    public SubMsFeatureCategory stageCategory(String feature, String stage) {
        Object f = root.get("features");
        if (!(f instanceof LinkedHashMap)) {
            return null;
        }
        Object e = ((LinkedHashMap<String, Object>) f).get(feature);
        if (!(e instanceof LinkedHashMap)) {
            return null;
        }
        Object sts = ((LinkedHashMap<String, Object>) e).get("stages");
        if (!(sts instanceof LinkedHashMap)) {
            return null;
        }
        Object row = ((LinkedHashMap<String, Object>) sts).get(stage);
        if (!(row instanceof LinkedHashMap)) {
            return null;
        }
        Object v = ((LinkedHashMap<String, Object>) row).get("perf");
        return v instanceof String ? SubMsFeatureCategory.fromWire((String) v) : null;
    }

    /**
     * Provenance recorded for a single feature, falling back to the file-level
     * stamp for a manifest written before per-feature provenance existed.
     *
     * <p>Prefer this over {@link #p99Source()}: a merge-written manifest can hold
     * features from different runs, and only this distinguishes them.
     */
    @SuppressWarnings("unchecked")
    public SubMsP99Source featureP99Source(String name) {
        Object f = root.get("features");
        if (f instanceof LinkedHashMap) {
            Object e = ((LinkedHashMap<String, Object>) f).get(name);
            if (e instanceof LinkedHashMap) {
                Object v = ((LinkedHashMap<String, Object>) e).get("p99Source");
                if (v instanceof String) {
                    return SubMsP99Source.fromWire((String) v);
                }
            }
        }
        // An absent stamp means two different things, and conflating them
        // reintroduces the bug this exists to prevent. In a PRE-V2 manifest
        // nothing is stamped and the file-level field is the only provenance
        // there is. In a v2 manifest, where some features ARE stamped, an
        // unstamped one is a feature this run did not measure - handing it the
        // file's stamp is exactly how a local figure came to sit inside a file
        // marked `fleet`. Report null and let the caller treat unknown as
        // unpublishable.
        if (f instanceof LinkedHashMap) {
            for (Object v : ((LinkedHashMap<String, Object>) f).values()) {
                if (v instanceof LinkedHashMap
                        && ((LinkedHashMap<String, Object>) v).containsKey("p99Source")) {
                    return null;
                }
            }
        }
        Object top = root.get("p99_source");
        return top instanceof String ? SubMsP99Source.fromWire((String) top) : null;
    }

    /** The rating currently recorded for a feature, if any. */
    @SuppressWarnings("unchecked")
    public SubMsFeatureCategory categoryOf(String name) {
        Object f = root.get("features");
        if (!(f instanceof LinkedHashMap)) {
            return null;
        }
        Object e = ((LinkedHashMap<String, Object>) f).get(name);
        if (!(e instanceof LinkedHashMap)) {
            return null;
        }
        Object perf = ((LinkedHashMap<String, Object>) e).get("perf");
        return perf instanceof String ? SubMsFeatureCategory.fromWire((String) perf) : null;
    }

    /** The manifest serialised back to pretty JSON. */
    public String toJson() {
        StringBuilder sb = new StringBuilder();
        writePretty(root, sb, 0);
        sb.append('\n');
        return sb.toString();
    }

    /**
     * Load from a file, preserving all fields. A missing/unreadable file yields an
     * empty manifest for {@code lang} (never an error - the first write creates it).
     */
    public static SubMsFeatureManifest load(String lang, Path path) {
        try {
            if (Files.exists(path)) {
                return loadStr(lang, Files.readString(path));
            }
        } catch (IOException e) {
            // fall through to an empty manifest
        }
        return create(lang);
    }

    /**
     * Write the manifest to {@code path}, creating parent directories. The write
     * always emits a full, valid document (never a partial file).
     */
    public void save(Path path) throws IOException {
        if (path.getParent() != null) {
            Files.createDirectories(path.getParent());
        }
        Files.writeString(path, toJson());
    }

    // ---------------- serialiser ----------------

    @SuppressWarnings("unchecked")
    private static void writePretty(Object v, StringBuilder out, int indent) {
        if (v == null) {
            out.append("null");
        } else if (v instanceof Boolean) {
            out.append(((Boolean) v) ? "true" : "false");
        } else if (v instanceof Num) {
            out.append(((Num) v).raw);
        } else if (v instanceof String) {
            writeString(out, (String) v);
        } else if (v instanceof List) {
            List<Object> items = (List<Object>) v;
            if (items.isEmpty()) {
                out.append("[]");
                return;
            }
            out.append("[\n");
            for (int i = 0; i < items.size(); i++) {
                pad(out, indent + 1);
                writePretty(items.get(i), out, indent + 1);
                if (i + 1 < items.size()) {
                    out.append(',');
                }
                out.append('\n');
            }
            pad(out, indent);
            out.append(']');
        } else if (v instanceof LinkedHashMap) {
            LinkedHashMap<String, Object> obj = (LinkedHashMap<String, Object>) v;
            if (obj.isEmpty()) {
                out.append("{}");
                return;
            }
            out.append("{\n");
            int i = 0, n = obj.size();
            for (Map.Entry<String, Object> e : obj.entrySet()) {
                pad(out, indent + 1);
                writeString(out, e.getKey());
                out.append(": ");
                writePretty(e.getValue(), out, indent + 1);
                if (++i < n) {
                    out.append(',');
                }
                out.append('\n');
            }
            pad(out, indent);
            out.append('}');
        } else {
            // Fallback for any Long/Double a caller injected directly.
            out.append(v.toString());
        }
    }

    private static void pad(StringBuilder out, int indent) {
        for (int i = 0; i < indent; i++) {
            out.append("  ");
        }
    }

    private static void writeString(StringBuilder out, String s) {
        out.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"':
                    out.append("\\\"");
                    break;
                case '\\':
                    out.append("\\\\");
                    break;
                case '\n':
                    out.append("\\n");
                    break;
                case '\r':
                    out.append("\\r");
                    break;
                case '\t':
                    out.append("\\t");
                    break;
                default:
                    if (c < 0x20) {
                        out.append(String.format("\\u%04x", (int) c));
                    } else {
                        out.append(c);
                    }
            }
        }
        out.append('"');
    }

    /** A JSON number kept as its literal text so precision round-trips exactly. */
    static final class Num {
        final String raw;

        Num(String raw) {
            this.raw = raw;
        }
    }

    // ---------------- parser ----------------

    /** Shared zero-dep JSON entry points, reused by {@link SubMsBenchConfig}. */
    static Object parseJson(String text) {
        return new JsonParser(text.trim()).parse();
    }

    static String jsonToString(Object v) {
        StringBuilder sb = new StringBuilder();
        writePretty(v, sb, 0);
        sb.append('\n');
        return sb.toString();
    }

    static final class JsonParser {
        private final String s;
        private int pos;

        JsonParser(String s) {
            this.s = s;
            this.pos = 0;
        }

        Object parse() {
            skipWs();
            Object v = value();
            skipWs();
            if (pos != s.length()) {
                throw new IllegalStateException("trailing input at " + pos);
            }
            return v;
        }

        private char peek() {
            return pos < s.length() ? s.charAt(pos) : '\0';
        }

        private void skipWs() {
            while (pos < s.length()) {
                char c = s.charAt(pos);
                if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
                    pos++;
                } else {
                    break;
                }
            }
        }

        private Object value() {
            skipWs();
            char c = peek();
            switch (c) {
                case '{':
                    return object();
                case '[':
                    return array();
                case '"':
                    return string();
                case 't':
                case 'f':
                    return bool();
                case 'n':
                    return nullLit();
                default:
                    if (c == '-' || (c >= '0' && c <= '9')) {
                        return number();
                    }
                    throw new IllegalStateException("unexpected '" + c + "' at " + pos);
            }
        }

        private LinkedHashMap<String, Object> object() {
            pos++; // {
            LinkedHashMap<String, Object> out = new LinkedHashMap<>();
            skipWs();
            if (peek() == '}') {
                pos++;
                return out;
            }
            while (true) {
                skipWs();
                String key = string();
                skipWs();
                if (peek() != ':') {
                    throw new IllegalStateException("expected ':' at " + pos);
                }
                pos++;
                out.put(key, value());
                skipWs();
                char c = peek();
                if (c == ',') {
                    pos++;
                } else if (c == '}') {
                    pos++;
                    break;
                } else {
                    throw new IllegalStateException("expected ',' or '}' at " + pos);
                }
            }
            return out;
        }

        private List<Object> array() {
            pos++; // [
            List<Object> out = new ArrayList<>();
            skipWs();
            if (peek() == ']') {
                pos++;
                return out;
            }
            while (true) {
                out.add(value());
                skipWs();
                char c = peek();
                if (c == ',') {
                    pos++;
                } else if (c == ']') {
                    pos++;
                    break;
                } else {
                    throw new IllegalStateException("expected ',' or ']' at " + pos);
                }
            }
            return out;
        }

        private String string() {
            if (peek() != '"') {
                throw new IllegalStateException("expected string at " + pos);
            }
            pos++;
            StringBuilder sb = new StringBuilder();
            while (true) {
                if (pos >= s.length()) {
                    throw new IllegalStateException("unterminated string");
                }
                char c = s.charAt(pos);
                if (c == '"') {
                    pos++;
                    break;
                } else if (c == '\\') {
                    pos++;
                    char e = peek();
                    switch (e) {
                        case '"':
                            sb.append('"');
                            break;
                        case '\\':
                            sb.append('\\');
                            break;
                        case '/':
                            sb.append('/');
                            break;
                        case 'n':
                            sb.append('\n');
                            break;
                        case 't':
                            sb.append('\t');
                            break;
                        case 'r':
                            sb.append('\r');
                            break;
                        case 'b':
                            sb.append('\b');
                            break;
                        case 'f':
                            sb.append('\f');
                            break;
                        case 'u':
                            String hex = s.substring(pos + 1, pos + 5);
                            sb.append((char) Integer.parseInt(hex, 16));
                            pos += 4;
                            break;
                        default:
                            throw new IllegalStateException("bad escape at " + pos);
                    }
                    pos++;
                } else {
                    sb.append(c);
                    pos++;
                }
            }
            return sb.toString();
        }

        private Num number() {
            int start = pos;
            if (peek() == '-') {
                pos++;
            }
            while (pos < s.length()) {
                char c = s.charAt(pos);
                if ((c >= '0' && c <= '9') || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-') {
                    pos++;
                } else {
                    break;
                }
            }
            return new Num(s.substring(start, pos));
        }

        private Boolean bool() {
            if (s.startsWith("true", pos)) {
                pos += 4;
                return Boolean.TRUE;
            }
            if (s.startsWith("false", pos)) {
                pos += 5;
                return Boolean.FALSE;
            }
            throw new IllegalStateException("bad literal at " + pos);
        }

        private Object nullLit() {
            if (s.startsWith("null", pos)) {
                pos += 4;
                return null;
            }
            throw new IllegalStateException("bad literal at " + pos);
        }
    }
}
