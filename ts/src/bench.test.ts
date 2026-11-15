// Parity against the Rust crate. testdata/rust-golden.json is produced by
// rust/examples/ts_parity_golden.rs from fixed samples; each case is replayed
// here through the TS harness and must come out byte-identical.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, test } from "node:test";

import { assertP99Under, downsample, printSummary, summarize, summaryToJson } from "./bench.ts";
import { formatNs, toFixedHalfEven } from "./format.ts";
import { SubMsPerfHarness } from "./harness.ts";
import { type SubMsBenchSummary } from "./summary.ts";

interface GoldenCase {
  name: string;
  sample_cap: number | null;
  stages: { name: string; samples: number[] }[];
  summary_json: string;
  print_summary: string;
  assert_at_p99: string | null;
  assert_below_p99: string | null;
  assert_below_p99_harness: string | null;
  assert_missing: string | null;
}

interface GoldenStage {
  count: number;
  p50_ns: number;
  p99_ns: number;
  p999_ns: number;
  max_ns: number;
  mean_ns: number;
  stddev_ns: number;
  jitter_score: number;
  cdf_buckets_ns: number[];
  samples_ns: number[];
}

interface GoldenRun {
  timestamp: string;
  meta: Record<string, string>;
  cpu: { core: number | null; affinity: string | null } | null;
  stages: Record<string, GoldenStage>;
}

const golden = JSON.parse(
  readFileSync(new URL("../testdata/rust-golden.json", import.meta.url), "utf8"),
) as { cases: GoldenCase[]; format_ns: { inputs: number[]; outputs: string[] } };

const STAGE_KEYS = [
  "count",
  "p50_ns",
  "p99_ns",
  "p999_ns",
  "max_ns",
  "mean_ns",
  "stddev_ns",
  "jitter_score",
  "cdf_buckets_ns",
  "samples_ns",
];

// Mirrors the harness setup in the Rust example, including the inputs that
// need escaping and sort out of insertion order.
function replay(c: GoldenCase, rust: GoldenRun): { h: SubMsPerfHarness; s: SubMsBenchSummary } {
  const h = new SubMsPerfHarness("ts-parity", "rust");
  if (c.sample_cap !== null) h.setSampleCap(c.sample_cap);
  h.input("b_key", "plain");
  h.input("a_key", "quote\" back\\ nl\n tab\t ctl\u0001");
  h.addMeta("case", c.name);
  h.addMeta("harness_version", rust.meta.harness_version);
  for (const st of c.stages) {
    const stage = h.stage(st.name, st.samples.length);
    for (const v of st.samples) stage.record(v);
  }
  // Wall clock and core placement are the only fields that vary per run.
  const s = summarize(h);
  s.timestamp = rust.timestamp;
  s.cpuCore = rust.cpu?.core ?? null;
  s.cpuAffinity = rust.cpu?.affinity ?? null;
  return { h, s };
}

function errorOf(fn: () => void): string | null {
  try {
    fn();
    return null;
  } catch (e) {
    return (e as Error).message;
  }
}

describe("parity with the Rust harness", () => {
  for (const c of golden.cases) {
    const rust = JSON.parse(c.summary_json) as GoldenRun;

    test(`${c.name}: every stats field matches`, () => {
      const { s } = replay(c, rust);
      assert.deepEqual(
        s.stages.map((st) => st.name),
        Object.keys(rust.stages),
      );
      for (const st of s.stages) {
        const want = rust.stages[st.name];
        assert.deepEqual(Object.keys(want), STAGE_KEYS);
        assert.equal(st.count, want.count, "count");
        assert.equal(st.p50Ns, want.p50_ns, "p50_ns");
        assert.equal(st.p99Ns, want.p99_ns, "p99_ns");
        assert.equal(st.p999Ns, want.p999_ns, "p999_ns");
        assert.equal(st.maxNs, want.max_ns, "max_ns");
        assert.equal(st.meanNs, want.mean_ns, "mean_ns");
        assert.equal(st.stddevNs, want.stddev_ns, "stddev_ns");
        assert.equal(Number(toFixedHalfEven(st.jitterScore, 4)), want.jitter_score, "jitter_score");
        assert.deepEqual(st.cdfBucketsNs, want.cdf_buckets_ns, "cdf_buckets_ns");
        assert.deepEqual(st.samplesNs, want.samples_ns, "samples_ns");
      }
    });

    test(`${c.name}: summaryToJson is byte-identical`, () => {
      assert.equal(summaryToJson(replay(c, rust).s), c.summary_json);
    });

    test(`${c.name}: printSummary is byte-identical`, () => {
      let text = "";
      printSummary(replay(c, rust).s, { write: (chunk) => (text += chunk) });
      assert.equal(text, c.print_summary);
    });

    test(`${c.name}: assertP99Under messages match`, () => {
      const { h, s } = replay(c, rust);
      const first = c.stages[0].name;
      const p99 = rust.stages[first].p99_ns;
      const below = Math.max(0, p99 - 1);
      assert.equal(errorOf(() => assertP99Under(s, [{ stage: first, p99NsMax: p99 }])), c.assert_at_p99);
      assert.equal(errorOf(() => assertP99Under(s, [{ stage: first, p99NsMax: below }])), c.assert_below_p99);
      assert.equal(
        errorOf(() => assertP99Under(h, [{ stage: first, p99NsMax: below }])),
        c.assert_below_p99_harness,
      );
      assert.equal(errorOf(() => assertP99Under(s, [{ stage: "nope", p99NsMax: 0 }])), c.assert_missing);
    });
  }

  test("formatNs matches format_ns, ties included", () => {
    assert.deepEqual(golden.format_ns.inputs.map(formatNs), golden.format_ns.outputs);
  });
});

describe("summarize", () => {
  test("stamps harness_version and sorts inputs and meta by key", () => {
    const h = new SubMsPerfHarness("w", "typescript");
    h.input("z", "1").input("a", "2").addMeta("m", "3");
    const json = summaryToJson(summarize(h));
    assert.match(json, /"inputs":\{"a":"2","z":"1"\},"meta":\{"harness_version":"[^"]+","m":"3"\}/);
    assert.ok(json.endsWith("}}\n"));
  });

  test("timestamp is UTC to the second", () => {
    assert.match(summarize(new SubMsPerfHarness("w", "ts")).timestamp, /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$/);
  });

  test("sorts map keys by code point, as BTreeMap does", () => {
    const h = new SubMsPerfHarness("w", "ts");
    // U+1F600 is a surrogate pair starting 0xD83D, below U+FF01 in UTF-16 but above it by code point.
    h.input("\u{1F600}", "a").input("\uFF01", "b");
    assert.match(summaryToJson(summarize(h)), /"inputs":\{"\uFF01":"b","\u{1F600}":"a"\}/u);
  });
});

describe("downsample", () => {
  test("keeps every step-th point and can exceed the cap", () => {
    const xs = Array.from({ length: 10 }, (_, i) => i);
    assert.deepEqual(downsample(xs, 4), [0, 2, 4, 6, 8]);
    assert.deepEqual(downsample(xs, 100), xs);
    assert.deepEqual(downsample(xs, 0), [0]);
    assert.deepEqual(downsample([], 5), []);
  });
});
