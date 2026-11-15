import assert from "node:assert/strict";
import { test } from "node:test";

import { formatNs, toFixedHalfEven } from "./format.ts";

test("exact ties round to even, unlike toFixed", () => {
  assert.equal(toFixedHalfEven(0.03125, 4), "0.0312");
  assert.equal((0.03125).toFixed(4), "0.0313");
  assert.equal(toFixedHalfEven(0.15625, 4), "0.1562");
  assert.equal(toFixedHalfEven(1.25, 1), "1.2");
  assert.equal(toFixedHalfEven(1.75, 1), "1.8");
  assert.equal(toFixedHalfEven(0.5, 0), "0");
  assert.equal(toFixedHalfEven(1.5, 0), "2");
});

test("non-ties follow the exact binary value", () => {
  // 1.05 is stored slightly above 1.05 and 1.15 slightly below 1.15.
  assert.equal(toFixedHalfEven(1.05, 1), "1.1");
  assert.equal(toFixedHalfEven(1.15, 1), "1.1");
  assert.equal(toFixedHalfEven(0, 4), "0.0000");
  assert.equal(toFixedHalfEven(1, 4), "1.0000");
  assert.equal(toFixedHalfEven(5e-324, 4), "0.0000");
  assert.equal(toFixedHalfEven(2 ** 60, 2), "1152921504606846976.00");
});

test("formatNs picks the unit by magnitude", () => {
  assert.equal(formatNs(999), "999ns");
  assert.equal(formatNs(1_250), "1.2us");
  assert.equal(formatNs(999_999), "1000.0us");
  assert.equal(formatNs(1_125_000), "1.12ms");
});
