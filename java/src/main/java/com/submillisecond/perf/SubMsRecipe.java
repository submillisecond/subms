package com.submillisecond.perf;

/** A standardised perf recipe driven through a {@link SubMsPerfHarness}. Mirrors the Rust `SubMsRecipe` trait. */
public interface SubMsRecipe {

    /** Workload name as it appears in the harness JSON output. */
    String name();

    /** Run the recipe's stages, populating {@code h} with timed samples. */
    void run(SubMsPerfHarness h, SubMsBenchParams params);
}
