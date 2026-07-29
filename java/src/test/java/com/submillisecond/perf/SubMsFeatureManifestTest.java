package com.submillisecond.perf;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;

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
}
