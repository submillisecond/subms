// Fixed-point formatting that matches Rust's `{:.N}`. Both round the exact
// binary value of the double, but on an exact tie Rust picks the even digit
// while Number.prototype.toFixed rounds up: 0.03125 is "0.0312" in Rust and
// "0.0313" from toFixed. Ties are real here (a jitter score of 1/32, a 1250 ns
// latency printed as 1.25 us), so the rounding is done in BigInt.

const view = new DataView(new ArrayBuffer(8));

/** `x` to `digits` decimals, rounding half to even on the exact value. `x` must be finite and >= 0. */
export function toFixedHalfEven(x: number, digits: number): string {
  view.setFloat64(0, x);
  const hi = view.getUint32(0);
  const lo = view.getUint32(4);
  const exp = (hi >>> 20) & 0x7ff;
  let mant = (BigInt(hi & 0xfffff) << 32n) | BigInt(lo);
  // Normal doubles carry an implicit leading 1; subnormals use exponent 1.
  let e: number;
  if (exp === 0) {
    e = 1 - 1075;
  } else {
    mant |= 1n << 52n;
    e = exp - 1075;
  }
  // x = mant * 2^e, so x * 10^digits = mant * 10^digits * 2^e.
  const scaled = mant * 10n ** BigInt(digits);
  let q: bigint;
  if (e >= 0) {
    q = scaled << BigInt(e);
  } else {
    const den = 1n << BigInt(-e);
    q = scaled / den;
    const twice = (scaled % den) * 2n;
    if (twice > den || (twice === den && (q & 1n) === 1n)) q += 1n;
  }
  const s = q.toString();
  if (digits === 0) return s;
  const padded = s.padStart(digits + 1, "0");
  return `${padded.slice(0, -digits)}.${padded.slice(-digits)}`;
}

/** Compact unit-aware ns formatter: ns below 1 us, us to one decimal below 1 ms, ms to two decimals above. */
export function formatNs(ns: number): string {
  if (ns < 1_000) return `${ns}ns`;
  if (ns < 1_000_000) return `${toFixedHalfEven(ns / 1_000, 1)}us`;
  return `${toFixedHalfEven(ns / 1_000_000, 2)}ms`;
}
