//! Shared utilities.

/// Deterministic LCG. Repeatable, zero-deps. Not cryptographic.
pub struct SubMsLcg(u64);

impl SubMsLcg {
    pub fn new(seed: u64) -> Self {
        // OR-1 so seed=0 doesn't collapse the sequence.
        SubMsLcg(seed | 1)
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
