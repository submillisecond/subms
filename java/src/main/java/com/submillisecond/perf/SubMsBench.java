package com.submillisecond.perf;

import java.io.IOException;
import java.io.PrintStream;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

/**
 * Shared bench helpers. The pipeline is:
 *
 * <pre>
 *   recipe -> SubMsPerfHarness -> SubMsBenchSummary -> { print, assert, JSON }
 * </pre>
 *
 * <p>{@link #summarize} converts the raw harness into a typed
 * {@link SubMsBenchSummary} you can hold, return, assert against, or hand to
 * downstream tools. {@link #printSummary} and {@link #summaryToJson} are
 * separate presenters on top of that data; they never compute new stats.
 *
 * <p>The Rust crate ships the same surface ({@code subms::summarize},
 * {@code subms::print_summary}, {@code subms::summary_to_json}) with
 * byte-equivalent output, so downstream tooling can consume either runtime.
 */
public final class SubMsBench {

    private SubMsBench() {}

    // ---------------------------------------------------------------------
    // Summarise (structured) -- the canonical analyser
    // ---------------------------------------------------------------------

    /** Build a {@link SubMsBenchSummary} from the harness. Includes the
     *  downsampled (max-500) per-stage samples in registration order. */
    public static SubMsBenchSummary summarize(SubMsPerfHarness h) {
        return summarizeInternal(h, /*includeSamples*/ true);
    }

    /** Same as {@link #summarize} but drops the per-stage sample arrays.
     *  Use when you only need count + percentiles + mean. */
    public static SubMsBenchSummary summarizeLean(SubMsPerfHarness h) {
        return summarizeInternal(h, /*includeSamples*/ false);
    }

    private static SubMsBenchSummary summarizeInternal(SubMsPerfHarness h, boolean includeSamples) {
        List<SubMsStageSummary> stageList = new ArrayList<>();
        for (SubMsPerfHarness.Stage stage : h.stagesInOrder()) {
            stageList.add(summarizeStage(stage, includeSamples));
        }
        return new SubMsBenchSummary(
                h.workload(),
                h.lang(),
                h.timestamp(),
                copyOf(h.inputs()),
                copyOf(h.meta()),
                List.copyOf(stageList));
    }

    private static SubMsStageSummary summarizeStage(SubMsPerfHarness.Stage stage, boolean includeSamples) {
        long[] chronological = stage.samples();   // already a defensive copy
        long[] sorted = chronological.clone();
        Arrays.sort(sorted);
        int n = sorted.length;
        long p50  = percentile(sorted, 0.50);
        long p99  = percentile(sorted, 0.99);
        long p999 = percentile(sorted, 0.999);
        long max  = n == 0 ? 0 : sorted[n - 1];
        long sum  = 0;
        for (long x : sorted) sum += x;
        long mean = n == 0 ? 0 : sum / n;
        Optional<long[]> samples = includeSamples
                ? Optional.of(downsample(chronological))
                : Optional.empty();
        return new SubMsStageSummary(stage.name(), n, p50, p99, p999, max, mean, samples);
    }

    /** Evenly-spaced downsample to at most 500 points, chronological order preserved. */
    static long[] downsample(long[] chronological) {
        int n = chronological.length;
        if (n == 0) return new long[0];
        int step = Math.max(1, n / 500);
        int outLen = (n + step - 1) / step;
        long[] out = new long[outLen];
        int oi = 0;
        for (int i = 0; i < n; i += step) {
            out[oi++] = chronological[i];
        }
        return Arrays.copyOf(out, oi);
    }

    private static Map<String, String> copyOf(Map<String, String> src) {
        return new LinkedHashMap<>(src);
    }

    // ---------------------------------------------------------------------
    // Print (presenter)
    // ---------------------------------------------------------------------

    /** Print a fixed-width percentile table for every stage in the summary,
     *  in registration order. Byte-equivalent to {@code subms::print_summary}. */
    public static void printSummary(SubMsBenchSummary s, PrintStream out) {
        out.printf("  %-9s  %9s  %9s  %9s  %9s  %9s%n",
                "stage", "p50", "p99", "p99.9", "max", "mean");
        for (SubMsStageSummary stage : s.stages()) {
            out.printf("  %-9s  %9s  %9s  %9s  %9s  %9s%n",
                    stage.name(),
                    formatNs(stage.p50Ns()),
                    formatNs(stage.p99Ns()),
                    formatNs(stage.p999Ns()),
                    formatNs(stage.maxNs()),
                    formatNs(stage.meanNs()));
        }
    }

    /** Convenience: summarise + print in one call. */
    public static void printSummary(SubMsPerfHarness h, PrintStream out) {
        printSummary(summarizeLean(h), out);
    }

    /** Compact unit-aware ns formatter. Matches Rust {@code subms::format_ns}.
     *  Sub-microsecond stays in ns; sub-millisecond goes to us with one
     *  decimal; everything else is ms to two decimals. */
    public static String formatNs(long ns) {
        if (ns < 1_000)     return String.format("%dns",   ns);
        if (ns < 1_000_000) return String.format("%.1fus", ns / 1_000.0);
        return String.format("%.2fms", ns / 1_000_000.0);
    }

    // ---------------------------------------------------------------------
    // Assert
    // ---------------------------------------------------------------------

    /** A single stage-level p99 assertion. */
    public record Assertion(
            /** Stage name as registered with the harness. */ String stage,
            /** Upper bound, ns. */ long p99NsMax) {}

    /** Asserts each stage's p99 stays under its bound. Throws
     *  {@link AssertionError} on the first violation or missing stage. */
    public static void assertP99Under(SubMsBenchSummary s, List<Assertion> assertions) {
        for (Assertion a : assertions) {
            SubMsStageSummary stage = s.stage(a.stage())
                    .orElseThrow(() -> new AssertionError(
                            "stage '" + a.stage() + "' not found in summary"));
            if (stage.p99Ns() > a.p99NsMax()) {
                throw new AssertionError(
                        "stage '" + a.stage() + "' p99 = " + stage.p99Ns()
                                + " ns exceeded limit " + a.p99NsMax() + " ns");
            }
        }
    }

    /** Back-compat overload: summarise the harness, then assert. */
    public static void assertP99Under(SubMsPerfHarness h, List<Assertion> assertions) {
        assertP99Under(summarizeLean(h), assertions);
    }

    // ---------------------------------------------------------------------
    // Run
    // ---------------------------------------------------------------------

    /** Run a recipe through a fresh harness and return it populated.
     *  Matches Rust's {@code subms::run_bench}. */
    public static SubMsPerfHarness runBench(SubMsRecipe r, SubMsBenchParams p) {
        SubMsPerfHarness h = new SubMsPerfHarness(r.name(), "java");
        h.input("entries", Integer.toString(p.entries()));
        h.input("warmup", Integer.toString(p.warmup()));
        h.input("seed", Long.toString(p.seed()));
        r.run(h, p);
        return h;
    }

    // ---------------------------------------------------------------------
    // JSON (presenter)
    // ---------------------------------------------------------------------

    /** Serialise the summary to the standard subms JSON shape.
     *  Byte-equivalent to {@code subms::summary_to_json}. */
    public static void summaryToJson(SubMsBenchSummary s, PrintStream out) throws IOException {
        StringBuilder buf = new StringBuilder(65_536);
        appendSummaryJson(buf, s);
        out.write(buf.toString().getBytes(StandardCharsets.UTF_8));
        out.write('\n');
        out.flush();
    }

    /** Variant for non-PrintStream sinks (e.g. file writers, mock buffers). */
    public static void summaryToJson(SubMsBenchSummary s, Writer out) throws IOException {
        StringBuilder buf = new StringBuilder(65_536);
        appendSummaryJson(buf, s);
        out.write(buf.toString());
        out.write('\n');
        out.flush();
    }

    static void appendSummaryJson(StringBuilder out, SubMsBenchSummary s) {
        out.append('{');
        kv(out, "workload", s.workload());  out.append(',');
        kv(out, "lang",     s.lang());      out.append(',');
        kv(out, "timestamp", s.timestamp()); out.append(',');
        out.append("\"inputs\":"); map(out, s.inputs()); out.append(',');
        out.append("\"meta\":");   map(out, s.meta());   out.append(',');
        out.append("\"stages\":{");
        boolean first = true;
        for (SubMsStageSummary stage : s.stages()) {
            if (!first) out.append(',');
            first = false;
            jsonStr(out, stage.name());
            out.append(':');
            stageJson(out, stage);
        }
        out.append("}}");
    }

    private static void stageJson(StringBuilder out, SubMsStageSummary stage) {
        out.append('{')
           .append("\"count\":").append(stage.count()).append(',')
           .append("\"p50_ns\":").append(stage.p50Ns()).append(',')
           .append("\"p99_ns\":").append(stage.p99Ns()).append(',')
           .append("\"p999_ns\":").append(stage.p999Ns()).append(',')
           .append("\"max_ns\":").append(stage.maxNs()).append(',')
           .append("\"mean_ns\":").append(stage.meanNs()).append(',')
           .append("\"samples_ns\":[");
        if (stage.samplesNs().isPresent()) {
            long[] arr = stage.samplesNs().get();
            for (int i = 0; i < arr.length; i++) {
                if (i > 0) out.append(',');
                out.append(arr[i]);
            }
        }
        out.append("]}");
    }

    private static void jsonStr(StringBuilder out, String v) {
        out.append('"');
        for (int i = 0; i < v.length(); i++) {
            char c = v.charAt(i);
            switch (c) {
                case '"':  out.append("\\\""); break;
                case '\\': out.append("\\\\"); break;
                case '\n': out.append("\\n");  break;
                case '\r': out.append("\\r");  break;
                case '\t': out.append("\\t");  break;
                default:
                    if (c < 0x20) out.append(String.format("\\u%04x", (int) c));
                    else          out.append(c);
            }
        }
        out.append('"');
    }

    private static void kv(StringBuilder out, String k, String v) {
        jsonStr(out, k); out.append(':'); jsonStr(out, v);
    }

    private static void map(StringBuilder out, Map<String, String> m) {
        out.append('{');
        boolean first = true;
        for (Map.Entry<String, String> e : m.entrySet()) {
            if (!first) out.append(',');
            first = false;
            kv(out, e.getKey(), e.getValue());
        }
        out.append('}');
    }

    // ---------------------------------------------------------------------
    // Sweep (multi-run varied-input pipeline)
    // ---------------------------------------------------------------------

    /**
     * Run the recipe once per element of {@code paramsList}, summarise each
     * run, and bundle the summaries into a {@link SubMsBenchSweep}. The
     * supplied {@code variedInputKey} should name the input that differs
     * across runs (typically {@code "entries"} for scale studies); pass
     * {@code null} or empty to leave it unset.
     */
    public static SubMsBenchSweep runSweep(SubMsRecipe r, List<SubMsBenchParams> paramsList, String variedInputKey) {
        List<SubMsBenchSummary> runs = new ArrayList<>(paramsList.size());
        for (SubMsBenchParams p : paramsList) {
            runs.add(summarize(runBench(r, p)));
        }
        return new SubMsBenchSweep(
                r.name(),
                "java",
                Optional.ofNullable(variedInputKey == null || variedInputKey.isEmpty() ? null : variedInputKey),
                List.copyOf(runs));
    }

    /** Bundle a pre-computed list of summaries (e.g. captured separately) into
     *  a sweep. All summaries should share a {@code workload}; the first
     *  summary's workload is used. */
    public static SubMsBenchSweep summarizeSweep(List<SubMsBenchSummary> summaries, String variedInputKey) {
        if (summaries.isEmpty()) {
            throw new IllegalArgumentException("summarizeSweep requires at least one run");
        }
        SubMsBenchSummary first = summaries.get(0);
        return new SubMsBenchSweep(
                first.workload(),
                first.lang(),
                Optional.ofNullable(variedInputKey == null || variedInputKey.isEmpty() ? null : variedInputKey),
                List.copyOf(summaries));
    }

    /**
     * Print a pivoted percentile table per stage: one block per stage, one row
     * per run, labelled by the varied input value (or by ordinal if no varied
     * key was supplied). Byte-equivalent to Rust's
     * {@code subms::print_sweep}.
     */
    public static void printSweep(SubMsBenchSweep sweep, PrintStream out) {
        if (sweep.runs().isEmpty()) {
            out.println("(empty sweep)");
            return;
        }
        // Gather stage names from the first run (assumed consistent).
        SubMsBenchSummary first = sweep.runs().get(0);
        Optional<String> varKey = sweep.variedInputKey();
        String header = varKey.orElse("run");
        for (SubMsStageSummary stage : first.stages()) {
            out.printf("stage: %s%n", stage.name());
            out.printf("  %-15s  %9s  %9s  %9s  %9s  %9s  %9s%n",
                    header, "count", "p50", "p99", "p99.9", "max", "mean");
            for (int idx = 0; idx < sweep.runs().size(); idx++) {
                SubMsBenchSummary run = sweep.runs().get(idx);
                String label;
                if (varKey.isPresent()) {
                    String v = run.inputs().get(varKey.get());
                    label = v == null ? "?" : v;
                } else {
                    label = "run " + (idx + 1);
                }
                Optional<SubMsStageSummary> s = run.stage(stage.name());
                if (s.isEmpty()) {
                    out.printf("  %-15s  (stage missing)%n", label);
                } else {
                    SubMsStageSummary ss = s.get();
                    out.printf("  %-15s  %9d  %9s  %9s  %9s  %9s  %9s%n",
                            label, ss.count(),
                            formatNs(ss.p50Ns()), formatNs(ss.p99Ns()),
                            formatNs(ss.p999Ns()), formatNs(ss.maxNs()), formatNs(ss.meanNs()));
                }
            }
            out.println();
        }
    }

    /** Emit a JSON array of run-summaries, identical shape to
     *  on-disk {@code perf/<lang>.json}. Byte-equivalent to Rust's
     *  {@code subms::sweep_to_json}. */
    public static void sweepToJson(SubMsBenchSweep sweep, PrintStream out) throws IOException {
        StringBuilder buf = new StringBuilder(65_536);
        buf.append('[');
        boolean first = true;
        for (SubMsBenchSummary run : sweep.runs()) {
            if (!first) buf.append(',');
            first = false;
            appendSummaryJson(buf, run);
        }
        buf.append(']');
        out.write(buf.toString().getBytes(StandardCharsets.UTF_8));
        out.write('\n');
        out.flush();
    }

    // ---------------------------------------------------------------------
    // Diff (baseline vs candidate regression detection)
    // ---------------------------------------------------------------------

    /** Diff result categorisation. The subms-perf-gate CI workflow fails the build
     *  when any stage's {@code worstRegressionPct} exceeds {@code regressionThresholdPct}. */
    /** Default threshold (percent) above which a stage's metric is considered
     *  a regression for CI-gate purposes. */
    public static final double DEFAULT_REGRESSION_THRESHOLD_PCT = 10.0;

    /** Build a typed diff between two summaries. Stages are matched by name;
     *  for matched stages, each metric (p50, p99, p99.9, max, mean) gets a
     *  per-metric delta. Use the default threshold (10 %) or supply your own
     *  via {@link #diffSummary(SubMsBenchSummary, SubMsBenchSummary, double)}. */
    public static SubMsBenchDiff diffSummary(SubMsBenchSummary baseline, SubMsBenchSummary candidate) {
        return diffSummary(baseline, candidate, DEFAULT_REGRESSION_THRESHOLD_PCT);
    }

    public static SubMsBenchDiff diffSummary(
            SubMsBenchSummary baseline,
            SubMsBenchSummary candidate,
            double regressionThresholdPct) {

        java.util.Set<String> baselineNames = new java.util.LinkedHashSet<>();
        for (SubMsStageSummary s : baseline.stages()) baselineNames.add(s.name());
        java.util.Set<String> candidateNames = new java.util.LinkedHashSet<>();
        for (SubMsStageSummary s : candidate.stages()) candidateNames.add(s.name());

        List<SubMsStageDiff> stageDiffs = new ArrayList<>();
        // Walk candidate's stage order so the output mirrors what the CI run produced.
        for (SubMsStageSummary cand : candidate.stages()) {
            Optional<SubMsStageSummary> base = baseline.stage(cand.name());
            if (base.isEmpty()) continue;   // candidate-only; recorded separately below
            stageDiffs.add(diffStage(base.get(), cand));
        }

        List<String> baselineOnly = new ArrayList<>();
        for (String n : baselineNames) if (!candidateNames.contains(n)) baselineOnly.add(n);
        List<String> candidateOnly = new ArrayList<>();
        for (String n : candidateNames) if (!baselineNames.contains(n)) candidateOnly.add(n);

        return new SubMsBenchDiff(
                baseline.workload(),
                candidate.workload(),
                candidate.lang(),
                List.copyOf(stageDiffs),
                List.copyOf(baselineOnly),
                List.copyOf(candidateOnly),
                regressionThresholdPct);
    }

    private static SubMsStageDiff diffStage(SubMsStageSummary baseline, SubMsStageSummary candidate) {
        List<SubMsMetricDiff> metrics = new ArrayList<>(5);
        metrics.add(metricDiff("p50",   baseline.p50Ns(),  candidate.p50Ns()));
        metrics.add(metricDiff("p99",   baseline.p99Ns(),  candidate.p99Ns()));
        metrics.add(metricDiff("p99.9", baseline.p999Ns(), candidate.p999Ns()));
        metrics.add(metricDiff("max",   baseline.maxNs(),  candidate.maxNs()));
        metrics.add(metricDiff("mean",  baseline.meanNs(), candidate.meanNs()));
        double worst = 0.0;
        for (SubMsMetricDiff m : metrics) {
            if (Double.isFinite(m.deltaPct()) && m.deltaPct() > worst) worst = m.deltaPct();
        }
        return new SubMsStageDiff(baseline.name(), List.copyOf(metrics), worst);
    }

    private static SubMsMetricDiff metricDiff(String name, long baseline, long candidate) {
        long delta = candidate - baseline;
        double deltaPct;
        if (baseline == 0) {
            deltaPct = candidate == 0 ? 0.0 : Double.POSITIVE_INFINITY;
        } else {
            deltaPct = (100.0 * delta) / baseline;
        }
        return new SubMsMetricDiff(name, baseline, candidate, delta, deltaPct);
    }

    /** Print a regression-table view of the diff. Byte-equivalent to Rust's
     *  {@code subms::print_diff}. Renders the verdict column ({@code ok} /
     *  {@code REGRESSED}) keyed off {@code regressionThresholdPct}. */
    public static void printDiff(SubMsBenchDiff diff, PrintStream out) {
        out.printf("diff: %s vs %s (%s)  threshold=%+.1f%%%n",
                diff.baselineWorkload(), diff.candidateWorkload(), diff.lang(),
                diff.regressionThresholdPct());
        out.printf("  %-12s  %-7s  %9s  %9s  %9s  %9s  %s%n",
                "stage", "metric", "baseline", "candidate", "delta", "%delta", "verdict");
        for (SubMsStageDiff stage : diff.stages()) {
            for (SubMsMetricDiff m : stage.metrics()) {
                String pctStr = Double.isFinite(m.deltaPct())
                        ? String.format("%+.1f%%", m.deltaPct())
                        : "+inf%";
                String verdict = (Double.isFinite(m.deltaPct()) && m.deltaPct() > diff.regressionThresholdPct())
                        ? "REGRESSED" : "ok";
                String deltaStr = (m.deltaNs() >= 0 ? "+" : "") + formatNs(Math.abs(m.deltaNs()));
                if (m.deltaNs() < 0) deltaStr = "-" + formatNs(Math.abs(m.deltaNs()));
                out.printf("  %-12s  %-7s  %9s  %9s  %9s  %9s  %s%n",
                        stage.stage(), m.metric(),
                        formatNs(m.baselineNs()), formatNs(m.candidateNs()),
                        deltaStr, pctStr, verdict);
            }
        }
        if (!diff.baselineOnlyStages().isEmpty()) {
            out.printf("  stages only in baseline:  %s%n", String.join(", ", diff.baselineOnlyStages()));
        }
        if (!diff.candidateOnlyStages().isEmpty()) {
            out.printf("  stages only in candidate: %s%n", String.join(", ", diff.candidateOnlyStages()));
        }
    }

    /** Emit the diff as a single JSON object for downstream tooling. Shape:
     *  <pre>
     *  {
     *    "baseline_workload": ..., "candidate_workload": ..., "lang": ...,
     *    "regression_threshold_pct": 10.0,
     *    "stages": [
     *      { "stage": "put", "worst_regression_pct": 25.0,
     *        "metrics": [ { "metric": "p99", "baseline_ns": 800, "candidate_ns": 1000,
     *                       "delta_ns": 200, "delta_pct": 25.0 }, ... ] }, ...
     *    ],
     *    "baseline_only_stages": [...],
     *    "candidate_only_stages": [...],
     *    "has_regression": false
     *  }
     *  </pre>
     *  Byte-equivalent to Rust's {@code subms::diff_to_json}. */
    public static void diffToJson(SubMsBenchDiff diff, PrintStream out) throws IOException {
        StringBuilder buf = new StringBuilder(8_192);
        appendDiffJson(buf, diff);
        out.write(buf.toString().getBytes(StandardCharsets.UTF_8));
        out.write('\n');
        out.flush();
    }

    static void appendDiffJson(StringBuilder out, SubMsBenchDiff diff) {
        out.append('{');
        kv(out, "baseline_workload", diff.baselineWorkload()); out.append(',');
        kv(out, "candidate_workload", diff.candidateWorkload()); out.append(',');
        kv(out, "lang", diff.lang()); out.append(',');
        out.append("\"regression_threshold_pct\":").append(diff.regressionThresholdPct()).append(',');
        out.append("\"has_regression\":").append(diff.hasRegression()).append(',');
        out.append("\"stages\":[");
        for (int i = 0; i < diff.stages().size(); i++) {
            if (i > 0) out.append(',');
            SubMsStageDiff s = diff.stages().get(i);
            out.append('{');
            kv(out, "stage", s.stage()); out.append(',');
            out.append("\"worst_regression_pct\":").append(jsonNumber(s.worstRegressionPct())).append(',');
            out.append("\"metrics\":[");
            for (int j = 0; j < s.metrics().size(); j++) {
                if (j > 0) out.append(',');
                SubMsMetricDiff m = s.metrics().get(j);
                out.append('{');
                kv(out, "metric", m.metric()); out.append(',');
                out.append("\"baseline_ns\":").append(m.baselineNs()).append(',');
                out.append("\"candidate_ns\":").append(m.candidateNs()).append(',');
                out.append("\"delta_ns\":").append(m.deltaNs()).append(',');
                out.append("\"delta_pct\":").append(jsonNumber(m.deltaPct()));
                out.append('}');
            }
            out.append("]}");
        }
        out.append("],");
        out.append("\"baseline_only_stages\":[");
        for (int i = 0; i < diff.baselineOnlyStages().size(); i++) {
            if (i > 0) out.append(',');
            jsonStr(out, diff.baselineOnlyStages().get(i));
        }
        out.append("],");
        out.append("\"candidate_only_stages\":[");
        for (int i = 0; i < diff.candidateOnlyStages().size(); i++) {
            if (i > 0) out.append(',');
            jsonStr(out, diff.candidateOnlyStages().get(i));
        }
        out.append("]}");
    }

    /** Render a finite double as a JSON number; non-finite values become null
     *  (JSON has no inf/NaN literal). */
    private static String jsonNumber(double d) {
        if (Double.isNaN(d) || Double.isInfinite(d)) return "null";
        return Double.toString(d);
    }

    // ---------------------------------------------------------------------
    // Numeric helpers
    // ---------------------------------------------------------------------

    /** Percentile over a sorted ns array. Empty -> 0. Index is
     *  {@code min(n-1, floor(q*n))} so q=1.0 returns the max. */
    public static long percentile(long[] sorted, double q) {
        if (sorted.length == 0) return 0;
        int idx = Math.min(sorted.length - 1, (int) (q * sorted.length));
        return sorted[idx];
    }
}
