//! Shared utilities.

/// Deterministic LCG. Repeatable, zero-deps. Not cryptographic.
pub struct SubMsLcg(u64);

impl SubMsLcg {
    pub fn new(seed: u64) -> Self {
        // The seed is the raw state. An earlier version OR-ed in 1 to "stop seed 0
        // collapsing", which it never did (the increment is odd, so the period is
        // 2^64 from any seed) and which aliased new(0) onto new(1). Recipes warm on
        // `seed` and measure on `seed + 1`, so at the default seed 0 that replayed
        // the warm-up keys as the measured ones.
        SubMsLcg(seed)
    }

    pub fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }

    pub fn bounded(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        self.next_u32() % n
    }
}

#[cfg(test)]
#[path = "util_tests.rs"]
mod tests;
