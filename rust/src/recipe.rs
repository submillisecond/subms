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
}

impl Default for SubMsBenchParams {
    fn default() -> Self {
        Self {
            entries: 50_000,
            warmup: 5_000,
            seed: 0,
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
    recipe.run(&mut h, params);
    h
}
