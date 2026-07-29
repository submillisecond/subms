//! `SubMsRecipe` trait + shared bench params.

use crate::SubMsPerfHarness;

/// Workload params. Each recipe interprets `entries` in its own units (bloom inserts,
/// LSM keys, etc).
#[derive(Debug, Clone)]
pub struct SubMsBenchParams {
    /// Work items per stage.
    pub entries: usize,
    /// Pre-timed warm-up iterations.
    pub warmup: usize,
    /// Deterministic RNG seed.
    pub seed: u64,
    /// Max points kept in each stage's emitted `samples_ns` timeline
    /// (evenly-spaced downsample of the full `entries` measurements).
    /// Default 500; raise it (e.g. 5000) when downstream tooling recomputes
    /// percentiles from the stored timeline and needs a denser sample.
    pub sample_cap: usize,
}

impl Default for SubMsBenchParams {
    fn default() -> Self {
        Self {
            entries: 50_000,
            warmup: 5_000,
            seed: 0,
            sample_cap: 500,
        }
    }
}

pub trait SubMsRecipe {
    fn name(&self) -> &str;
    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams);
}

pub fn benchmark<R: SubMsRecipe + ?Sized>(
    recipe: &R,
    params: &SubMsBenchParams,
) -> SubMsPerfHarness {
    let mut h = SubMsPerfHarness::new(recipe.name(), "rust");
    h.input("entries", &params.entries.to_string());
    h.input("warmup", &params.warmup.to_string());
    h.input("seed", &params.seed.to_string());
    h.set_sample_cap(params.sample_cap);
    recipe.run(&mut h, params);
    h
}

#[cfg(test)]
#[path = "recipe_tests.rs"]
mod tests;
