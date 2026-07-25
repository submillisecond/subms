//! Internal stats. **Not part of the public API.** The bench harness
//! needs `percentile`, `mean`, `stddev`, `cdf_buckets`, `jitter_score`
//! to compute the standard `SubMsStageSummary` and emit the JSON
//! contract; we duplicate the minimum here rather than depending on
//! the sibling `subms-stats` crate so that recipes downstream don't
//! pick up stats as a transitive dep.
//!
//! Consumers who want a rich statistics surface (CDF buckets, jitter,
//! tail analysis, KS, Cohen's d, bootstrap CIs, ...) should add
//! `subms-stats = "0.5"` directly. Recipes should depend only on
//! `subms`.
//!
//! Mirrors the implementations in `subms_stats::percentiles`,
//! `subms_stats::histogram`, `subms_stats::jitter`. Keep them
//! byte-equivalent: any divergence will surface as a JSON contract
//! mismatch between recipes (which see the subms internal version)
//! and external tooling (which uses subms-stats directly).

/// Percentile over a *sorted* ns array. Empty -> 0.
pub(crate) fn percentile(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((q * sorted.len() as f64) as usize).min(sorted.len() - 1);
    sorted[idx]
}

/// Arithmetic mean. `0` if empty.
pub(crate) fn mean(samples: &[u64]) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    samples.iter().sum::<u64>() / samples.len() as u64
}

/// Sample standard deviation (n-1 denominator). `0` when count < 2.
pub(crate) fn stddev(samples: &[u64]) -> u64 {
    let n = samples.len();
    if n < 2 {
        return 0;
    }
    let mean_f = mean(samples) as f64;
    let mut variance = 0.0f64;
    for &v in samples {
        let d = v as f64 - mean_f;
        variance += d * d;
    }
    variance /= (n - 1) as f64;
    variance.sqrt().round() as u64
}

/// Log2-spaced histogram of latencies. 64 buckets covering
/// `[2^i, 2^(i+1))` nanoseconds for `i in 0..64`.
pub(crate) fn cdf_buckets(samples: &[u64]) -> Vec<u64> {
    let mut buckets = vec![0u64; 64];
    for &v in samples {
        let idx = if v == 0 {
            0
        } else {
            (63 - v.leading_zeros()) as usize
        };
        let idx = idx.min(63);
        buckets[idx] = buckets[idx].saturating_add(1);
    }
    buckets
}

/// Jitter score: per-window CV across non-overlapping 32-sample
/// windows. Clamped to `[0.0, 1.0]`.
pub(crate) fn jitter_score(samples: &[u64]) -> f64 {
    const WIN: usize = 32;
    if samples.len() < WIN * 2 {
        return 0.0;
    }
    let windows = samples.len() / WIN;
    let mut means = Vec::with_capacity(windows);
    for w in 0..windows {
        let start = w * WIN;
        let slice = &samples[start..start + WIN];
        let sum: u64 = slice.iter().sum();
        means.push(sum as f64 / WIN as f64);
    }
    let grand_mean: f64 = means.iter().sum::<f64>() / means.len() as f64;
    if grand_mean <= 0.0 {
        return 0.0;
    }
    let variance: f64 =
        means.iter().map(|m| (m - grand_mean).powi(2)).sum::<f64>() / means.len() as f64;
    let cv = variance.sqrt() / grand_mean;
    cv.clamp(0.0, 1.0)
}
