package com.submillisecond.perf;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.TreeMap;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Mirrors Rust's {@code subms::feature} tests. */
final class SubMsFeatureManifestTest {

    private static long[][] sweep(long... pairs) {
        long[][] s = new long[pairs.length / 2][2];
        for (int i = 0; i < s.length; i++) {
            s[i][0] = pairs[i * 2];
            s[i][1] = pairs[i * 2 + 1];
        }
        return s;
    }

    private static Map<String, Long> p99(String stage, long ns) {
        Map<String, Long> m = new LinkedHashMap<>();
        m.put(stage, ns);
        return m;
    }

    @Test
    void classifiesFlatAboveBaseAsHotPath() {
        var d = SubMsFeatureManifest.classify(sweep(1_024, 300, 65_536, 320), 50L, null);
        assertEquals(SubMsFeatureCategory.HOT_PATH, d.category());
    }

    @Test
    void classifiesLinearGrowthAsStructural() {
        var d = SubMsFeatureManifest.classify(sweep(1_024, 1_000, 65_536, 64_000), null, null);
        assertEquals(SubMsFeatureCategory.STRUCTURAL, d.category());
    }

    @Test
    void logNGrowthStaysHotPath() {
        // ~6x p99 over 64x N - well under the 0.5*63 structural bar.
        var d = SubMsFeatureManifest.classify(sweep(1_024, 100, 65_536, 600), 50L, null);
        assertEquals(SubMsFeatureCategory.HOT_PATH, d.category());
    }

    @Test
    void noWorkloadIsAuxiliary() {
        var d = SubMsFeatureManifest.classify(new long[0][0], 300L, null);
        assertEquals(SubMsFeatureCategory.AUXILIARY, d.category());
    }

    @Test
    void noDeltaFromBaseIsAuxiliary() {
        var d = SubMsFeatureManifest.classify(sweep(1_024, 305, 65_536, 308), 300L, null);
        assertEquals(SubMsFeatureCategory.AUXILIARY, d.category());
    }

    @Test
    void overrideWins() {
        var d = SubMsFeatureManifest.classify(
                sweep(1_024, 1_000, 65_536, 64_000), null, SubMsFeatureCategory.HOT_PATH);
        assertEquals(SubMsFeatureCategory.HOT_PATH, d.category());
        assertTrue(d.reason().contains("override"));
    }

    @Test
    void mergePreservesDeepThirdPartyStructures() {
        String existing = """
            {
              "lang": "rust",
              "schemaVersion": 3,
              "vendor": { "name": "acme", "tags": ["a", "b"], "nested": { "deep": [1, 2, 3] } },
              "features": {
                "counting": {
                  "perf": "auxiliary",
                  "owner": "team-x",
                  "customArr": [{ "k": "v" }, 7, true, null],
                  "p99ByStage": { "add": 1 }
                },
                "scalable": { "perf": "hot-path", "vendorNote": "do not drop" }
              }
            }""";
        SubMsFeatureManifest m = SubMsFeatureManifest.loadStr("rust", existing);
        Map<String, Long> stages = new LinkedHashMap<>();
        stages.put("add", 320L);
        m.setFeature("counting", SubMsFeatureCategory.HOT_PATH, stages, "measured");
        String out = m.toJson();

        assertTrue(out.contains("\"schemaVersion\": 3"));
        assertTrue(out.contains("\"name\": \"acme\""));
        assertTrue(out.contains("\"deep\": ["));
        assertTrue(out.contains("\"customArr\""));
        assertTrue(out.contains("\"owner\": \"team-x\""));
        assertTrue(out.contains("null"));
        assertTrue(out.contains("true"));
        assertTrue(out.contains("\"vendorNote\": \"do not drop\""));
        assertTrue(out.contains("\"add\": 320"));
        assertEquals(SubMsFeatureCategory.HOT_PATH, m.categoryOf("counting"));
        assertEquals(SubMsFeatureCategory.HOT_PATH, m.categoryOf("scalable"));
    }

    @Test
    void repeatedMergesAreLossless() {
        SubMsFeatureManifest m =
                SubMsFeatureManifest.loadStr("rust", "{\"vendor\":{\"x\":1},\"features\":{}}");
        m.setFeature("a", SubMsFeatureCategory.HOT_PATH, null, "r");
        m.setFeature("b", SubMsFeatureCategory.STRUCTURAL, null, "r");
        m.setFeature("c", SubMsFeatureCategory.AUXILIARY, null, "r");
        m.setFeature("a", SubMsFeatureCategory.HOT_PATH, p99("op", 42L), "r2");
        String out = m.toJson();
        assertTrue(out.contains("\"vendor\""));
        assertTrue(out.contains("\"x\": 1"));
        assertEquals(SubMsFeatureCategory.HOT_PATH, m.categoryOf("a"));
        assertEquals(SubMsFeatureCategory.STRUCTURAL, m.categoryOf("b"));
        assertEquals(SubMsFeatureCategory.AUXILIARY, m.categoryOf("c"));
        assertTrue(out.contains("\"op\": 42"));
    }

    @Test
    void structuralFeatureDropsP99() {
        SubMsFeatureManifest m = SubMsFeatureManifest.create("rust");
        m.setFeature("serialize", SubMsFeatureCategory.HOT_PATH, p99("serialize", 5_000L), "x");
        m.setFeature("serialize", SubMsFeatureCategory.STRUCTURAL, null, "O(n)");
        String out = m.toJson();
        assertTrue(out.contains("\"perf\": \"structural\""));
        assertFalse(out.contains("p99ByStage"));
    }

    @Test
    void roundTripsNumberPrecision() {
        String existing = "{\"features\":{\"f\":{\"p99ByStage\":{\"add\":9007199254740993}}}}";
        SubMsFeatureManifest m = SubMsFeatureManifest.loadStr("rust", existing);
        assertTrue(m.toJson().contains("9007199254740993"));
    }

    @Test
    void sampleFeaturesClassifyCorrectly() {
        Long base = 300L;
        SubMsFeatureManifest m = SubMsFeatureManifest.create("rust");

        var counting = SubMsFeatureManifest.classify(
                sweep(1_024, 350, 16_384, 360, 262_144, 372), base, null);
        assertEquals(SubMsFeatureCategory.HOT_PATH, counting.category());

        var serialize = SubMsFeatureManifest.classify(
                sweep(1_024, 12_000, 16_384, 190_000, 262_144, 3_050_000), base, null);
        assertEquals(SubMsFeatureCategory.STRUCTURAL, serialize.category());

        var serde = SubMsFeatureManifest.classify(new long[0][0], base, null);
        assertEquals(SubMsFeatureCategory.AUXILIARY, serde.category());

        var metrics = SubMsFeatureManifest.classify(sweep(1_024, 308, 262_144, 312), base, null);
        assertEquals(SubMsFeatureCategory.AUXILIARY, metrics.category());

        var persistent = SubMsFeatureManifest.classify(
                sweep(1_024, 400, 262_144, 900), base, SubMsFeatureCategory.STRUCTURAL);
        assertEquals(SubMsFeatureCategory.STRUCTURAL, persistent.category());

        m.setFeature("counting", counting.category(), p99("op", 372L), counting.reason());
        m.setFeature("serialize", serialize.category(), null, serialize.reason());
        m.setFeature("serde", serde.category(), null, serde.reason());
        m.setFeature("metrics", metrics.category(), null, metrics.reason());
        m.setFeature("persistent", persistent.category(), null, persistent.reason());

        assertEquals(SubMsFeatureCategory.HOT_PATH, m.categoryOf("counting"));
        assertEquals(SubMsFeatureCategory.STRUCTURAL, m.categoryOf("serialize"));
        assertEquals(SubMsFeatureCategory.AUXILIARY, m.categoryOf("serde"));
        assertEquals(SubMsFeatureCategory.AUXILIARY, m.categoryOf("metrics"));
        assertEquals(SubMsFeatureCategory.STRUCTURAL, m.categoryOf("persistent"));
        assertNull(m.categoryOf("nonexistent"));
    }

    @Test
    void malformedInputNeverCrashesAndStaysWritable() {
        String[] bad = {"", "   ", "not json", "[1,2,3]", "{\"features\":", "42", "null", "\"a string\""};
        for (String b : bad) {
            SubMsFeatureManifest m = SubMsFeatureManifest.loadStr("rust", b);
            m.setFeature("f", SubMsFeatureCategory.HOT_PATH, null, "r");
            assertEquals(SubMsFeatureCategory.HOT_PATH, m.categoryOf("f"), "bad input: " + b);
            assertTrue(m.toJson().startsWith("{"));
        }
    }

    @Test
    void onlyTheTargetFeatureChanges() {
        String existing = """
            {"lang":"rust","features":{
                "a":{"perf":"hot-path","note":"A keep","p99ByStage":{"op":11}},
                "b":{"perf":"auxiliary","note":"B keep"}
            }}""";
        SubMsFeatureManifest m = SubMsFeatureManifest.loadStr("rust", existing);
        m.setFeature("b", SubMsFeatureCategory.HOT_PATH, p99("op", 22L), "measured");
        String out = m.toJson();
        assertTrue(out.contains("\"note\": \"A keep\""));
        assertTrue(out.contains("\"op\": 11"));
        assertTrue(out.contains("\"note\": \"B keep\""));
        assertTrue(out.contains("\"op\": 22"));
        assertEquals(SubMsFeatureCategory.HOT_PATH, m.categoryOf("a"));
    }

    @Test
    void reclassifyKeepsCustomFieldsDropsP99() {
        String existing = "{\"features\":{\"f\":{\"perf\":\"hot-path\",\"owner\":\"me\",\"p99ByStage\":{\"op\":5}}}}";
        SubMsFeatureManifest m = SubMsFeatureManifest.loadStr("rust", existing);
        m.setFeature("f", SubMsFeatureCategory.STRUCTURAL, null, "O(n)+");
        String out = m.toJson();
        assertTrue(out.contains("\"owner\": \"me\""));
        assertFalse(out.contains("p99ByStage"));
        assertTrue(out.contains("\"perf\": \"structural\""));
    }

    @Test
    void setFeatureIsIdempotent() {
        SubMsFeatureManifest a = SubMsFeatureManifest.loadStr("rust", "{\"vendor\":1,\"features\":{}}");
        SubMsFeatureManifest b = SubMsFeatureManifest.loadStr("rust", "{\"vendor\":1,\"features\":{}}");
        a.setFeature("f", SubMsFeatureCategory.HOT_PATH, p99("op", 7L), "r");
        b.setFeature("f", SubMsFeatureCategory.HOT_PATH, p99("op", 7L), "r");
        b.setFeature("f", SubMsFeatureCategory.HOT_PATH, p99("op", 7L), "r");
        assertEquals(a.toJson(), b.toJson());
    }

    @Test
    void degenerateFeaturesPreservesOtherTopLevelKeys() {
        String existing = "{\"lang\":\"rust\",\"vendor\":{\"keep\":true},\"features\":42}";
        SubMsFeatureManifest m = SubMsFeatureManifest.loadStr("rust", existing);
        m.setFeature("f", SubMsFeatureCategory.HOT_PATH, null, "r");
        String out = m.toJson();
        assertTrue(out.contains("\"keep\": true"));
        assertEquals(SubMsFeatureCategory.HOT_PATH, m.categoryOf("f"));
    }

    @Test
    void loadMissingThenSaveCreatesFileAndDirs(@TempDir Path dir) throws IOException {
        Path path = dir.resolve("nested").resolve("features").resolve("rust.json");
        assertFalse(Files.exists(path));
        SubMsFeatureManifest m = SubMsFeatureManifest.load("rust", path);
        m.setFeature("f", SubMsFeatureCategory.HOT_PATH, p99("op", 7L), "r");
        m.save(path);
        assertTrue(Files.exists(path));
        SubMsFeatureManifest reloaded = SubMsFeatureManifest.load("rust", path);
        assertEquals(SubMsFeatureCategory.HOT_PATH, reloaded.categoryOf("f"));
    }

    @Test
    void parserCoversAllValueKindsAndEscapes() {
        // Every value kind + every string escape (unicode + a control char) +
        // signed/decimal/exponent numbers, driven through a load -> re-serialise
        // round-trip so both the parser and the serialiser arms are exercised.
        String rich = "{\"lang\":\"rust\","
                + "\"str\":\"a\\\"b\\\\c\\/d\\n\\t\\r\\b\\f\\u0041\\u0002\","
                + "\"neg\":-12,\"exp\":1.5e3,\"flt\":3.14,"
                + "\"bt\":true,\"bf\":false,\"nul\":null,"
                + "\"arr\":[1,\"x\",true,null,[2],{\"k\":3}],\"empty\":[],"
                + "\"features\":{}}";
        SubMsFeatureManifest m = SubMsFeatureManifest.loadStr("rust", rich);
        String out = m.toJson();
        // Number literals round-trip verbatim (parser keeps the raw token).
        assertTrue(out.contains("-12"));
        assertTrue(out.contains("1.5e3"));
        assertTrue(out.contains("3.14"));
        assertTrue(out.contains("\"arr\""));
        // The parsed control char re-escapes via the \\uXXXX serialiser arm.
        assertTrue(out.contains("\\u0002"));
        // The serialised form re-parses (nested arrays/objects survive the round-trip).
        SubMsFeatureManifest round = SubMsFeatureManifest.loadStr("rust", out);
        assertTrue(round.toJson().contains("\"neg\""));
        // A reason carrying escapes exercises the writeString escape arms.
        m.setFeature("f", SubMsFeatureCategory.HOT_PATH, p99("op", 1L), "l1\nl2\t\"q\"\\z");
        assertTrue(m.toJson().contains("\\n"));
    }

    @Test
    void parserErrorBranchesFallBackNotCrash() {
        // Each malformed input drives a distinct parser throw; loadStr must catch
        // it and fall back to a fresh, writable manifest.
        String[] bad = {
            "{\"a\" 1}",         // expected ':'
            "{\"a\":1 \"b\":2}", // expected ',' or '}'
            "[1 2]",             // expected ',' or ']'
            "\"unterminated",    // unterminated string
            "tru",               // bad bool literal
            "nul",               // bad null literal
            "{}x",               // trailing input
            "\"\\x\"",           // bad escape
            "@",                 // unexpected leading byte
        };
        for (String b : bad) {
            SubMsFeatureManifest m = SubMsFeatureManifest.loadStr("rust", b);
            m.setFeature("f", SubMsFeatureCategory.HOT_PATH, null, "r");
            assertEquals(SubMsFeatureCategory.HOT_PATH, m.categoryOf("f"), "input: " + b);
        }
    }

    @Test
    void categoryEnumWireForms() {
        assertEquals("hot-path", SubMsFeatureCategory.HOT_PATH.asString());
        assertEquals("structural", SubMsFeatureCategory.STRUCTURAL.asString());
        assertEquals("auxiliary", SubMsFeatureCategory.AUXILIARY.asString());
        assertEquals(SubMsFeatureCategory.HOT_PATH, SubMsFeatureCategory.fromWire("hot-path"));
        assertEquals(SubMsFeatureCategory.STRUCTURAL, SubMsFeatureCategory.fromWire("structural"));
        assertEquals(SubMsFeatureCategory.AUXILIARY, SubMsFeatureCategory.fromWire("auxiliary"));
        assertNull(SubMsFeatureCategory.fromWire("nope"));
        assertNull(SubMsFeatureCategory.fromWire(null));
    }

    // ---------------- p99 provenance ----------------

    @Test
    void unstampedManifestReadsAsLocal() {
        // The conservative direction: an unattributed number is withheld, not
        // published as if it came from the conformance box.
        SubMsFeatureManifest m = SubMsFeatureManifest.create("java");
        assertEquals(SubMsP99Source.LOCAL, m.p99Source());
        assertNull(m.p99SourceRef());
    }

    @Test
    void fleetStampRecordsSourceAndInstance() {
        SubMsFeatureManifest m = SubMsFeatureManifest.create("java");
        m.setP99Source(SubMsP99Source.FLEET, "i-0abc123def456");
        assertEquals(SubMsP99Source.FLEET, m.p99Source());
        assertEquals("i-0abc123def456", m.p99SourceRef());
        String json = m.toJson();
        assertTrue(json.contains("\"p99_source\": \"fleet\""), json);
        assertTrue(json.contains("\"p99_source_ref\": \"i-0abc123def456\""), json);
    }

    @Test
    void localStampClearsAStaleFleetReference() {
        // The case that matters: a manifest captured on the box, then re-run on a
        // laptop, must not keep claiming the box.
        SubMsFeatureManifest m = SubMsFeatureManifest.create("java");
        m.setP99Source(SubMsP99Source.FLEET, "i-0abc123def456");
        m.setP99Source(SubMsP99Source.LOCAL, null);
        assertEquals(SubMsP99Source.LOCAL, m.p99Source());
        assertNull(m.p99SourceRef());
        assertFalse(m.toJson().contains("p99_source_ref"));
    }

    @Test
    void aReferencePassedWithLocalIsIgnored() {
        SubMsFeatureManifest m = SubMsFeatureManifest.create("java");
        m.setP99Source(SubMsP99Source.LOCAL, "i-0abc123def456");
        assertNull(m.p99SourceRef());
    }

    @Test
    void anEmptyFleetReferenceIsNotRecorded() {
        // An empty instance id is an absent one; writing it would look like
        // provenance while identifying nothing.
        SubMsFeatureManifest m = SubMsFeatureManifest.create("java");
        m.setP99Source(SubMsP99Source.FLEET, "");
        assertEquals(SubMsP99Source.FLEET, m.p99Source());
        assertNull(m.p99SourceRef());
    }

    @Test
    void stampRoundTripsThroughLoadStrAndPreservesOtherFields() {
        SubMsFeatureManifest m = SubMsFeatureManifest.create("java");
        m.setP99Source(SubMsP99Source.FLEET, "i-1");
        Map<String, Long> p99 = new TreeMap<>();
        p99.put("get", 900L);
        m.setFeature("compaction", SubMsFeatureCategory.HOT_PATH, p99, "flat");
        SubMsFeatureManifest reloaded = SubMsFeatureManifest.loadStr("java", m.toJson());
        assertEquals(SubMsP99Source.FLEET, reloaded.p99Source());
        assertEquals("i-1", reloaded.p99SourceRef());
        assertEquals(SubMsFeatureCategory.HOT_PATH, reloaded.categoryOf("compaction"));
    }

    @Test
    void anUnknownSourceTokenReadsAsLocal() {
        // A typo withholds numbers instead of publishing them.
        SubMsFeatureManifest m =
                SubMsFeatureManifest.loadStr(
                        "java", "{\"lang\":\"java\",\"p99_source\":\"ec2\",\"features\":{}}");
        assertEquals(SubMsP99Source.LOCAL, m.p99Source());
        assertEquals(SubMsP99Source.FLEET, SubMsP99Source.fromWire("fleet"));
        assertEquals(SubMsP99Source.LOCAL, SubMsP99Source.fromWire("local"));
        assertEquals(SubMsP99Source.LOCAL, SubMsP99Source.fromWire(null));
        assertEquals("fleet", SubMsP99Source.FLEET.asString());
        assertEquals("local", SubMsP99Source.LOCAL.asString());
    }

    @Test
    void restampingKeepsTheKeyInPlace() {
        // LinkedHashMap.put preserves position, so a re-stamp must not shuffle
        // the document - matching the Rust port's set().
        SubMsFeatureManifest m = SubMsFeatureManifest.create("java");
        m.setP99Source(SubMsP99Source.LOCAL, null);
        int first = m.toJson().indexOf("p99_source");
        m.setP99Source(SubMsP99Source.FLEET, "i-2");
        assertEquals(first, m.toJson().indexOf("p99_source"));
    }

    @Test
    void aFeatureFasterThanBaseIsNotReportedAsWithinTheDelta() {
        // The auxiliary branch fires for anything at or below base, so a feature
        // a third of the baseline was described as "within 10% of base" - a
        // recorded reason that is simply false, on the one field that audits the
        // category.
        long[][] sweep = {{1024, 200}, {65536, 200}};
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, 700L, null);
        assertEquals(SubMsFeatureCategory.AUXILIARY, d.category());
        assertTrue(d.reason().contains("at or below base"), d.reason());
        assertFalse(d.reason().contains("within"), d.reason());
    }

    @Test
    void aFeatureJustAboveBaseStillReadsAsWithinTheDelta() {
        // 730 is above base 700 but inside the 10% band - the genuine non-effect.
        long[][] sweep = {{1024, 730}, {65536, 730}};
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, 700L, null);
        assertEquals(SubMsFeatureCategory.AUXILIARY, d.category());
        assertTrue(d.reason().contains("within 10% of base"), d.reason());
    }

    // ---- v2: the claim line and the undecidable band (mirrors the Rust suite) ----

    @Test
    void aFlatOpAboveTheClaimLineIsReportedNotClaimed() {
        // adaptive-radix-tree/serialize measured 30.7 ms flat and was published as
        // a per-op sub-ms claim, because nothing bounded the hot-path branch.
        long[][] sweep = {{4_096L, 30_000_000L}, {262_144L, 30_674_448L}};
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, 1_000L, null);
        assertEquals(SubMsFeatureCategory.REPORTED, d.category());
        assertTrue(d.reason().contains("claim line"), d.reason());
    }

    @Test
    void theClaimLineDoesNotSwallowAGenuineSubMsHotPath() {
        long[][] sweep = {{4_096L, 900_000L}, {262_144L, 950_000L}};
        assertEquals(
                SubMsFeatureCategory.HOT_PATH,
                SubMsFeatureManifest.classify(sweep, 100L, null).category());
    }

    @Test
    void aFeatureCostingAboutTheGuardIsIndeterminateNotACoinToss() {
        // block-cache/metrics, both fleet runs: 269ns vs base 246 read auxiliary,
        // 272ns vs base 245 read hot-path - a 3ns move flipping the category.
        assertEquals(
                SubMsFeatureCategory.INDETERMINATE,
                SubMsFeatureManifest.classify(new long[][] {{1L, 269L}}, 246L, null).category());
        assertEquals(
                SubMsFeatureCategory.INDETERMINATE,
                SubMsFeatureManifest.classify(new long[][] {{1L, 272L}}, 245L, null).category());
    }

    @Test
    void exactlyBaseStaysAuxiliaryAndIsNeverIndeterminate() {
        // The band is on the EXCESS, not the guard - banding the guard made this
        // indeterminate, which is wrong: zero delta is unambiguously auxiliary.
        assertEquals(
                SubMsFeatureCategory.AUXILIARY,
                SubMsFeatureManifest.classify(new long[][] {{1L, 300L}}, 300L, null).category());
    }

    @Test
    void aSweepStraddlingTheStructuralGuardIsIndeterminate() {
        long[][] sweep = {{4_096L, 1_000L}, {262_144L, 35_000L}};
        SubMsFeatureManifest.Decision d = SubMsFeatureManifest.classify(sweep, 100L, null);
        assertEquals(SubMsFeatureCategory.INDETERMINATE, d.category());
        assertTrue(d.reason().contains("too close to call"), d.reason());
    }

    @Test
    void aClearlySuperlinearSweepIsStillStructural() {
        long[][] sweep = {{4_096L, 540_000L}, {262_144L, 71_776_000L}};
        assertEquals(
                SubMsFeatureCategory.STRUCTURAL,
                SubMsFeatureManifest.classify(sweep, null, null).category());
    }

    @Test
    void everyCategoryRoundTripsThroughItsWireValue() {
        for (SubMsFeatureCategory c : SubMsFeatureCategory.values()) {
            assertEquals(c, SubMsFeatureCategory.fromWire(c.asString()));
        }
    }

    // ---- v2: per-feature provenance (mirrors the Rust suite) ----

    @Test
    void aFeatureWrittenThisRunCarriesThisRunsProvenance() {
        SubMsFeatureManifest m = SubMsFeatureManifest.create("java");
        m.setP99Source(SubMsP99Source.FLEET, "i-07f269c7f5d290fc4");
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("get", 300L);
        m.setFeature("counting", SubMsFeatureCategory.HOT_PATH, p99, "why");
        assertEquals(SubMsP99Source.FLEET, m.featureP99Source("counting"));
        assertTrue(m.toJson().contains("\"p99Source\""), m.toJson());
        assertTrue(m.toJson().contains("i-07f269c7f5d290fc4"), m.toJson());
    }

    @Test
    void aFeatureTheRunDidNotTouchKeepsItsOwnProvenance() {
        // The manifest MERGE-writes, so a feature not re-measured keeps its old
        // numbers. Under a file-level stamp alone it silently inherited the new
        // one - a local figure inside a `fleet` file.
        SubMsFeatureManifest first = SubMsFeatureManifest.create("java");
        first.setP99Source(SubMsP99Source.LOCAL, null);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("op", 100L);
        first.setFeature("serde", SubMsFeatureCategory.AUXILIARY, p99, "local run");

        SubMsFeatureManifest second = SubMsFeatureManifest.loadStr("java", first.toJson());
        second.setP99Source(SubMsP99Source.FLEET, "i-abc123ff");
        second.setFeature("counting", SubMsFeatureCategory.HOT_PATH, p99, "fleet run");

        assertEquals(SubMsP99Source.FLEET, second.featureP99Source("counting"));
        assertEquals(
                SubMsP99Source.LOCAL,
                second.featureP99Source("serde"),
                "a carried-over feature must NOT inherit the fleet stamp");
    }

    @Test
    void aPreV2ManifestFallsBackToTheFileLevelStamp() {
        String legacy =
                "{\"lang\":\"java\",\"p99_source\":\"fleet\",\"features\":{\"x\":{\"perf\":\"hot-path\"}}}";
        SubMsFeatureManifest m = SubMsFeatureManifest.loadStr("java", legacy);
        assertEquals(SubMsP99Source.FLEET, m.featureP99Source("x"));
    }

    @Test
    void reRunningAFeatureLocallyClearsItsFleetReference() {
        SubMsFeatureManifest m = SubMsFeatureManifest.create("java");
        m.setP99Source(SubMsP99Source.FLEET, "i-abc123ff");
        Map<String, Long> empty = new LinkedHashMap<>();
        m.setFeature("x", SubMsFeatureCategory.AUXILIARY, empty, "fleet");
        m.setP99Source(SubMsP99Source.LOCAL, null);
        m.setFeature("x", SubMsFeatureCategory.AUXILIARY, empty, "local");
        assertEquals(SubMsP99Source.LOCAL, m.featureP99Source("x"));
        assertTrue(!m.toJson().contains("i-abc123ff"), "stale fleet ref survived a local re-run");
    }

    // ---- v2: per-stage classification (mirrors the Rust suite) ----

    private static SubMsFeatureManifest.StageClass st(long p99, SubMsFeatureCategory c) {
        return new SubMsFeatureManifest.StageClass(p99, c, c.asString());
    }

    @Test
    void aMixedFeatureRollsUpToItsMostRestrictiveStage() {
        // adaptive-radix-tree/concurrent-reads: a 1227ns get and a 44ms snapshot,
        // published under ONE hot-path label.
        Map<String, SubMsFeatureManifest.StageClass> stages = new LinkedHashMap<>();
        stages.put("get", st(1_227L, SubMsFeatureCategory.HOT_PATH));
        stages.put("snapshot", st(44_037_700L, SubMsFeatureCategory.REPORTED));
        assertEquals(SubMsFeatureCategory.REPORTED, SubMsFeatureManifest.rollUpStages(stages));
    }

    @Test
    void anAllHotFeatureStillRollsUpHot() {
        Map<String, SubMsFeatureManifest.StageClass> stages = new LinkedHashMap<>();
        stages.put("add", st(60L, SubMsFeatureCategory.HOT_PATH));
        stages.put("contains", st(58L, SubMsFeatureCategory.HOT_PATH));
        assertEquals(SubMsFeatureCategory.HOT_PATH, SubMsFeatureManifest.rollUpStages(stages));
    }

    @Test
    void auxiliaryStagesNeverDragAHotFeatureDown() {
        Map<String, SubMsFeatureManifest.StageClass> stages = new LinkedHashMap<>();
        stages.put("probe", st(60L, SubMsFeatureCategory.HOT_PATH));
        stages.put("stats", st(10L, SubMsFeatureCategory.AUXILIARY));
        assertEquals(SubMsFeatureCategory.HOT_PATH, SubMsFeatureManifest.rollUpStages(stages));
    }

    @Test
    void oneIndeterminateStageMakesTheFeatureIndeterminate() {
        Map<String, SubMsFeatureManifest.StageClass> stages = new LinkedHashMap<>();
        stages.put("a", st(100L, SubMsFeatureCategory.STRUCTURAL));
        stages.put("b", st(100L, SubMsFeatureCategory.INDETERMINATE));
        assertEquals(SubMsFeatureCategory.INDETERMINATE, SubMsFeatureManifest.rollUpStages(stages));
    }

    @Test
    void setFeatureStagesWritesPerStageDetailAndTheRollup() {
        SubMsFeatureManifest m = SubMsFeatureManifest.create("java");
        m.setP99Source(SubMsP99Source.FLEET, "i-abc123ff");
        Map<String, SubMsFeatureManifest.StageClass> stages = new LinkedHashMap<>();
        stages.put("get", st(1_227L, SubMsFeatureCategory.HOT_PATH));
        stages.put("snapshot", st(44_037_700L, SubMsFeatureCategory.REPORTED));
        m.setFeatureStages("concurrent-reads", stages, "mixed");

        assertEquals(SubMsFeatureCategory.HOT_PATH, m.stageCategory("concurrent-reads", "get"));
        assertEquals(
                SubMsFeatureCategory.REPORTED, m.stageCategory("concurrent-reads", "snapshot"));
        assertEquals(SubMsP99Source.FLEET, m.featureP99Source("concurrent-reads"));
        assertTrue(m.toJson().contains("p99ByStage"), m.toJson());
    }

    @Test
    void anEmptyStageSetRollsUpToAuxiliary() {
        assertEquals(
                SubMsFeatureCategory.AUXILIARY,
                SubMsFeatureManifest.rollUpStages(new LinkedHashMap<>()));
    }
}
