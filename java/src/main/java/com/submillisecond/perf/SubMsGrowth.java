package com.submillisecond.perf;

import java.io.IOException;
import java.io.Writer;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

/**
 * Storage-growth capture: R rounds of timed ops, the footprint after each, and a
 * verdict on whether the curve stays inside its declared class.
 *
 * <p>Port of the Rust {@code growth} module. The emitted JSON is byte-equivalent
 * across the two ports, like every other contract in this harness.
 *
 * <p>This existed in Rust only, so no Java recipe could publish a growth curve
 * and the storage dimension was silently Rust-measured everywhere. The footprint
 * is RECIPE-SUPPLIED - {@link SubMsGrowthRecipe#diskBytes} and
 * {@link SubMsGrowthRecipe#memoryBytes} ask the structure how big it is, from
 * its own accounting. Nothing here reads the heap, so a Java curve is not a GC
 * artifact and is directly comparable with the Rust one: node count times node
 * size is the same number in both languages.
 */
public final class SubMsGrowth {

    /** Schema version of the emitted JSON. Matches the Rust GROWTH_VERSION. */
    public static final int GROWTH_VERSION = 2;

    private SubMsGrowth() {}

    /** How the footprint is expected to behave, and what that claim is gated on. */
    public enum GrowthClass {
        /** Footprint stays under an absolute byte ceiling. */
        BOUNDED("bounded"),
        /** Footprint stays within a multiple of live bytes. */
        AMPLIFICATION_BOUNDED("amplification_bounded"),
        /** Footprint flattens: the second half must not keep climbing. */
        PLATEAU_BOUNDED("plateau_bounded"),
        /** Growth is the design, not a leak. */
        UNBOUNDED_OK("unbounded_ok");

        private final String wire;

        GrowthClass(String wire) {
            this.wire = wire;
        }

        public String asString() {
            return wire;
        }
    }

    /** One round: the ops performed, the footprint after them, and the latencies. */
    public record Round(
            int round,
            int ops,
            int cumulativeOps,
            long diskBytes,
            long memoryBytes,
            long totalBytes,
            long liveBytes,
            double amplification,
            Map<String, Long> structures,
            long p50Ns,
            long p99Ns,
            long maxNs) {}

    /** The declared class, its bound, and whether the observed curve held. */
    public record Verdict(
            GrowthClass growthClass, double bound, boolean holds, double observed, String summary) {}

    /** A finished capture. */
    public record Report(
            String workload,
            String lang,
            String opName,
            List<Round> rounds,
            Verdict verdict,
            boolean compact,
            Map<String, String> meta) {}

    /** Run a growth recipe end to end. */
    public static Report grow(SubMsGrowthRecipe recipe, String lang) {
        int r = Math.max(1, recipe.rounds());
        int ops = Math.max(1, recipe.opsPerRound());
        List<Round> rounds = new ArrayList<>(r);
        int cumulative = 0;

        for (int round = 1; round <= r; round++) {
            long[] samples = new long[ops];
            for (int i = 0; i < ops; i++) {
                long t = System.nanoTime();
                recipe.op(round, i);
                samples[i] = System.nanoTime() - t;
            }
            recipe.endRound(round);
            cumulative += ops;
            java.util.Arrays.sort(samples);

            long disk = recipe.diskBytes();
            long memory = recipe.memoryBytes();
            long total = disk + memory;
            long live = recipe.liveBytes();
            double amplification = live > 0 ? (double) total / (double) live : 0.0;

            rounds.add(new Round(
                    round,
                    ops,
                    cumulative,
                    disk,
                    memory,
                    total,
                    live,
                    amplification,
                    new TreeMap<>(recipe.structures()),
                    SubMsBench.percentile(samples, 0.50),
                    SubMsBench.percentile(samples, 0.99),
                    samples.length == 0 ? 0 : samples[samples.length - 1]));
        }

        return new Report(
                recipe.name(),
                lang,
                recipe.opName(),
                rounds,
                computeVerdict(recipe.expectedClass(), recipe.expectedBound(), rounds),
                recipe.compact(),
                new LinkedHashMap<>());
    }

    private static Verdict computeVerdict(GrowthClass cls, double bound, List<Round> rounds) {
        switch (cls) {
            case BOUNDED -> {
                long peak = rounds.stream().mapToLong(Round::totalBytes).max().orElse(0L);
                return new Verdict(cls, bound, peak <= bound, peak,
                        "peak footprint " + peak + " bytes vs bound " + (long) bound + " bytes");
            }
            case AMPLIFICATION_BOUNDED -> {
                double amp = rounds.stream()
                        .filter(x -> x.liveBytes() > 0)
                        .mapToDouble(Round::amplification)
                        .max()
                        .orElse(0.0);
                return new Verdict(cls, bound, amp <= bound, amp,
                        "max footprint/live amplification " + fixed2(amp)
                                + "x vs ceiling " + fixed2(bound) + "x");
            }
            case PLATEAU_BOUNDED -> {
                // Baseline at the MID-POINT, not round 1: a store with a warm-up
                // ramp is small early and that is not growth. What must stay flat
                // is the second half - still climbing from mid-run to the end is
                // what a leak looks like.
                int n = rounds.size();
                long midBytes = n == 0 ? 0 : rounds.get(n / 2).totalBytes();
                double mid = Math.max(1L, midBytes);
                double last = n == 0 ? 0.0 : rounds.get(n - 1).totalBytes();
                double ratio = last / mid;
                return new Verdict(cls, bound, ratio <= bound, ratio,
                        "footprint grew " + fixed2(ratio) + "x from mid-run to round " + n
                                + " (ceiling " + fixed2(bound) + "x)");
            }
            default -> {
                return new Verdict(cls, bound, true, 0.0, "growth expected and unbounded by design");
            }
        }
    }

    /** CI gate: null when the curve holds, else the breach summary. */
    public static String assertGrowthHolds(Report report) {
        return report.verdict().holds()
                ? null
                : report.workload() + " growth gate breached: " + report.verdict().summary();
    }

    // ---- JSON. Byte-equivalent with the Rust emitter; field order is part of
    // the contract, not a formatting preference. ----

    public static void growthToJson(Report report, Writer out) throws IOException {
        StringBuilder s = new StringBuilder(256 + report.rounds().size() * 128);
        s.append('{');
        s.append("\"kind\":\"growth\",");
        kvStr(s, "workload", report.workload());
        s.append(',');
        kvStr(s, "lang", report.lang());
        s.append(',');
        kvStr(s, "op", report.opName());
        s.append(',');
        s.append("\"growth_version\":").append(GROWTH_VERSION).append(',');

        s.append("\"verdict\":{");
        kvStr(s, "class", report.verdict().growthClass().asString());
        s.append(',');
        s.append("\"bound\":").append(fixed4(report.verdict().bound())).append(',');
        s.append("\"holds\":").append(report.verdict().holds()).append(',');
        s.append("\"observed\":").append(fixed4(report.verdict().observed())).append(',');
        kvStr(s, "summary", report.verdict().summary());
        s.append("},");

        s.append("\"compact\":").append(report.compact()).append(',');

        s.append("\"rounds\":[");
        for (int i = 0; i < report.rounds().size(); i++) {
            Round r = report.rounds().get(i);
            if (i > 0) {
                s.append(',');
            }
            s.append('{');
            s.append("\"round\":").append(r.round()).append(',');
            s.append("\"ops\":").append(r.ops()).append(',');
            s.append("\"cumulative_ops\":").append(r.cumulativeOps()).append(',');
            s.append("\"disk_bytes\":").append(r.diskBytes()).append(',');
            s.append("\"memory_bytes\":").append(r.memoryBytes()).append(',');
            s.append("\"total_bytes\":").append(r.totalBytes()).append(',');
            s.append("\"live_bytes\":").append(r.liveBytes()).append(',');
            s.append("\"amplification\":").append(fixed4(r.amplification())).append(',');
            s.append("\"structures\":{");
            List<Map.Entry<String, Long>> entries = new ArrayList<>(r.structures().entrySet());
            entries.sort(Comparator.comparing(Map.Entry::getKey));
            for (int j = 0; j < entries.size(); j++) {
                if (j > 0) {
                    s.append(',');
                }
                jsonStr(s, entries.get(j).getKey());
                s.append(':').append(entries.get(j).getValue());
            }
            s.append("},");
            s.append("\"p50_ns\":").append(r.p50Ns()).append(',');
            s.append("\"p99_ns\":").append(r.p99Ns()).append(',');
            s.append("\"max_ns\":").append(r.maxNs());
            s.append('}');
        }
        s.append(']');

        if (!report.meta().isEmpty()) {
            s.append(",\"meta\":{");
            List<Map.Entry<String, String>> m = new ArrayList<>(report.meta().entrySet());
            int i = 0;
            for (Map.Entry<String, String> e : m) {
                if (i++ > 0) {
                    s.append(',');
                }
                jsonStr(s, e.getKey());
                s.append(':');
                jsonStr(s, e.getValue());
            }
            s.append('}');
        }

        s.append('}');
        out.write(s.toString());
    }

    private static String fixed4(double v) {
        return fixed(v, 4);
    }

    private static String fixed2(double v) {
        return fixed(v, 2);
    }

    /**
     * Rust's {@code {:.N}} - N decimals, no scientific notation, and half-to-EVEN
     * at the cut.
     *
     * <p>{@code String.format("%.4f", ..)} rounds half-UP, which diverges from
     * Rust on an exact tie. A footprint ratio is a quotient of byte counts, so a
     * dyadic value like 1568/1024 = 1.53125 is reachable and would have written
     * 1.5313 here against Rust's 1.5312 - a byte-equivalence break in the one
     * field the two ports are supposed to agree on. {@code new BigDecimal(double)}
     * takes the exact binary value, which is the same number Rust rounds from.
     */
    private static String fixed(double v, int scale) {
        if (Double.isNaN(v) || Double.isInfinite(v)) {
            return Double.isNaN(v) ? "NaN" : (v > 0 ? "inf" : "-inf");
        }
        return new java.math.BigDecimal(v)
                .setScale(scale, java.math.RoundingMode.HALF_EVEN)
                .toPlainString();
    }

    private static void kvStr(StringBuilder out, String k, String v) {
        jsonStr(out, k);
        out.append(':');
        jsonStr(out, v);
    }

    private static void jsonStr(StringBuilder out, String s) {
        out.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                default -> {
                    if (c < 0x20) {
                        out.append(String.format(java.util.Locale.ROOT, "\\u%04x", (int) c));
                    } else {
                        out.append(c);
                    }
                }
            }
        }
        out.append('"');
    }
}
