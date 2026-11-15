// Internal stats behind the stage summary. A port of the Rust crate's private
// `stats.rs`; every function must return what the Rust one returns for the
// same u64 samples, because the JSON from both feeds the same site. Samples
// are non-negative safe integers (enforced at record time). Sums can still
// pass 2^53, where doubles stop being exact, so the sums fall back to BigInt
// only when they get that big.

const SAFE = Number.MAX_SAFE_INTEGER;

/** Percentile over an ascending-sorted array. Nearest rank by truncation, no interpolation. Empty gives 0. */
export function percentile(sorted: ArrayLike<number>, q: number): number {
  const n = sorted.length;
  if (n === 0) return 0;
  // q * n is evaluated in f64 on both sides, so p999 picks the same index.
  return sorted[Math.min(Math.floor(q * n), n - 1)];
}

/** Integer mean, truncated (u64 division). Empty gives 0. */
export function mean(samples: readonly number[]): number {
  const n = samples.length;
  if (n === 0) return 0;
  let sum = 0;
  for (const v of samples) sum += v;
  // Samples are non-negative, so partial sums only grow: a final sum this far
  // under 2^53 means every partial sum was exact, and q * n stays exact too.
  if (sum < 2 ** 52) {
    let q = Math.floor(sum / n);
    if (q * n > sum) q -= 1;
    else if ((q + 1) * n <= sum) q += 1;
    return q;
  }
  let big = 0n;
  for (const v of samples) big += BigInt(v);
  return Number(big / BigInt(n));
}

/** Sample standard deviation (n - 1), rounded to the nearest ns. 0 when count < 2. */
export function stddev(samples: readonly number[]): number {
  const n = samples.length;
  if (n < 2) return 0;
  // Rust measures the spread around the already-truncated integer mean.
  const m = mean(samples);
  let variance = 0;
  for (const v of samples) {
    const d = v - m;
    variance += d * d;
  }
  variance /= n - 1;
  return Math.round(Math.sqrt(variance));
}

/** 64 log2 buckets: entry i counts samples in [2^i, 2^(i+1)) ns, with 0 counted in bucket 0. */
export function cdfBuckets(samples: readonly number[]): number[] {
  const buckets = new Array<number>(64).fill(0);
  for (const v of samples) buckets[log2Floor(v)] += 1;
  return buckets;
}

function log2Floor(v: number): number {
  if (v === 0) return 0;
  const hi = Math.floor(v / 2 ** 32);
  if (hi > 0) return 63 - Math.clz32(hi);
  return 31 - Math.clz32(v >>> 0);
}

const WIN = 32;

/**
 * Coefficient of variation of the per-window mean across non-overlapping
 * 32-sample windows, clamped to [0, 1]. 0 below two full windows. A trailing
 * partial window is ignored.
 */
export function jitterScore(samples: readonly number[]): number {
  if (samples.length < WIN * 2) return 0;
  const windows = Math.floor(samples.length / WIN);
  const means = new Array<number>(windows);
  for (let w = 0; w < windows; w++) {
    means[w] = windowSum(samples, w * WIN) / WIN;
  }
  // Plain left-to-right sums, in the same order as Rust's iterator sums, so
  // the rounding at each step matches.
  let total = 0;
  for (const m of means) total += m;
  const grand = total / windows;
  if (grand <= 0) return 0;
  let sq = 0;
  for (const m of means) sq += (m - grand) * (m - grand);
  const cv = Math.sqrt(sq / windows) / grand;
  return Math.min(Math.max(cv, 0), 1);
}

// Rust sums the window in u64 and converts once. Number(bigint) rounds to
// nearest, ties to even, the same as `as f64`.
function windowSum(samples: readonly number[], start: number): number {
  let sum = 0;
  for (let i = start; i < start + WIN; i++) sum += samples[i];
  if (sum <= SAFE) return sum;
  let big = 0n;
  for (let i = start; i < start + WIN; i++) big += BigInt(samples[i]);
  return Number(big);
}
