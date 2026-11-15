import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { HARNESS_VERSION, SubMsPerfHarness } from "./harness.ts";

test("HARNESS_VERSION tracks package.json", () => {
  const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")) as {
    version: string;
  };
  assert.equal(HARNESS_VERSION, pkg.version);
});

test("meta starts with harness_version and addMeta can override it", () => {
  const h = new SubMsPerfHarness("w", "ts");
  assert.equal(h.meta.get("harness_version"), HARNESS_VERSION);
  h.addMeta("harness_version", "x");
  assert.equal(h.meta.get("harness_version"), "x");
});

test("record rejects values outside u64-as-double", () => {
  const s = new SubMsPerfHarness("w", "ts").stage("s");
  s.record(0);
  s.record(Number.MAX_SAFE_INTEGER);
  for (const bad of [-1, 1.5, Number.NaN, Number.POSITIVE_INFINITY, 2 ** 53]) {
    assert.throws(() => s.record(bad), RangeError);
  }
  assert.deepEqual(s.samples, [0, Number.MAX_SAFE_INTEGER]);
});

test("time records one integer sample and passes the result through", () => {
  const s = new SubMsPerfHarness("w", "ts").stage("s", 1);
  assert.equal(s.time(() => 42), 42);
  assert.equal(s.samples.length, 1);
  assert.ok(Number.isSafeInteger(s.samples[0]) && s.samples[0] >= 0);
});

test("time records nothing when the call throws", () => {
  const s = new SubMsPerfHarness("w", "ts").stage("s");
  assert.throws(() =>
    s.time(() => {
      throw new Error("boom");
    }),
  );
  assert.equal(s.samples.length, 0);
});

test("timeAsync covers the awaited span", async () => {
  const s = new SubMsPerfHarness("w", "ts").stage("s");
  await s.timeAsync(() => new Promise((r) => setTimeout(r, 5)));
  assert.ok(s.samples[0] >= 4_000_000);
});

test("warmThenTime records only the measured pass", () => {
  const seen: number[] = [];
  const s = new SubMsPerfHarness("w", "ts").stage("s");
  s.warmThenTime(3, 2, (i) => seen.push(i));
  assert.deepEqual(seen, [0, 1, 2, 0, 1]);
  assert.equal(s.samples.length, 2);
});

test("stages keep registration order and allow duplicate names", () => {
  const h = new SubMsPerfHarness("w", "ts");
  h.stage("b");
  h.stage("a");
  h.stage("b");
  assert.deepEqual(
    h.stages.map((s) => s.name),
    ["b", "a", "b"],
  );
  assert.equal(h.stageByName("b"), h.stages[0]);
});

test("setSampleCap floors at 1", () => {
  assert.equal(new SubMsPerfHarness("w", "ts").sampleCap, 500);
  assert.equal(new SubMsPerfHarness("w", "ts").setSampleCap(0).sampleCap, 1);
});
