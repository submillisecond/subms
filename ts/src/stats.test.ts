import assert from "node:assert/strict";
import { test } from "node:test";

import { cdfBuckets, jitterScore, mean, percentile, stddev } from "./stats.ts";

test("percentile truncates q * n to an index and clamps to the last element", () => {
  const sorted = Array.from({ length: 1000 }, (_, i) => i + 1);
  assert.equal(percentile(sorted, 0.5), 501);
  assert.equal(percentile(sorted, 0.99), 991);
  assert.equal(percentile(sorted, 0.999), 1000);
  assert.equal(percentile([7], 0.999), 7);
  assert.equal(percentile([], 0.5), 0);
});

test("mean truncates like u64 division", () => {
  assert.equal(mean([1, 2]), 1);
  assert.equal(mean([]), 0);
});

test("mean stays exact once the sum passes 2^53", () => {
  const big = 2 ** 52 + 1;
  // 3 * (2^52 + 1) is odd and above 2^53, so a double sum would lose the low bit.
  assert.equal(mean([big, big, big]), big);
  assert.equal(mean([big, big, big + 2]), big);
});

test("stddev uses n - 1 around the truncated mean and rounds", () => {
  assert.equal(stddev([5]), 0);
  assert.equal(stddev([0, 1]), 1);
  assert.equal(stddev([2, 4, 4, 4, 5, 5, 7, 9]), 2);
});

test("cdfBuckets puts 0 and 1 in bucket 0 and uses floor(log2)", () => {
  const b = cdfBuckets([0, 1, 2, 3, 4, 2 ** 32, 2 ** 53 - 1]);
  assert.equal(b.length, 64);
  assert.equal(b[0], 2);
  assert.equal(b[1], 2);
  assert.equal(b[2], 1);
  assert.equal(b[32], 1);
  assert.equal(b[52], 1);
});

test("jitterScore needs two full windows and clamps to 1", () => {
  assert.equal(jitterScore(new Array(63).fill(10)), 0);
  assert.equal(jitterScore(new Array(64).fill(10)), 0);
  assert.equal(jitterScore(new Array(64).fill(0)), 0);
  const loud = [...new Array(31 * 32).fill(1), ...new Array(32).fill(1e9)];
  assert.equal(jitterScore(loud), 1);
});
