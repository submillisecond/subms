# subms (Rust)

The Rust side of the [subms](https://github.com/submillisecond/subms) perf
harness. Zero-dependency std-only crate. Records timed samples per stage,
computes percentiles, supports coordinated-omission correction, runs scale
sweeps, and emits a JSON contract byte-equivalent to the Java side.

See the [repo-root README](../README.md) for the cross-language overview,
JSON-contract spec, and ecosystem links.

## Install

```toml
[dependencies]
subms = "0.3"
```

## Quickstart

```rust
use subms::{
    SubMsPerfHarness, summarize, print_summary,
    assert_p99_under, SubMsBenchAssertion,
};

let mut h = SubMsPerfHarness::new("my-workload", "rust");
h.input("entries", "50000");

let put = h.stage("put", 50_000);
for _ in 0..50_000 {
    put.time(|| { /* hot-path work under test */ });
}

let summary = summarize(&h);
print_summary(&summary, &mut std::io::stdout())?;

assert_p99_under(
    &summary,
    &[SubMsBenchAssertion { stage: "put", p99_ns_max: 1_000_000 }],
)?;
```

## Feature flags

| flag | what it enables |
|---|---|
| (default) | The harness, summary, sweep, diff, timer, paced stage, JSON emission. Pure std-lib. |

No optional features today. The crate stays small and dependency-free on
purpose.

## Public API surface

| type / fn | does |
|---|---|
| `SubMsPerfHarness` | records timed samples per named stage |
| `SubMsStage` / `SubMsPacedStage` | per-stage recorder (paced = coordinated-omission corrected) |
| `summarize(&h)` / `summarize_lean(&h)` | sorted percentiles + downsampled timeline |
| `print_summary(&s, &mut writer)` | fixed-width percentile table |
| `summary_to_json(&s, &mut writer)` | the stable JSON shape |
| `run_sweep(&recipe, &[params...], varied_key)` | multi-run scale curve / feature toggle |
| `diff_summary(&base, &cand)` | typed per-stage regression delta |
| `assert_p99_under(&target, &[...])` | the cookbook quality gate |
| `SubMsTimer` | autostart stopwatch with named checkpoints |
| `SubMsRecipe` trait | implement for cookbook-style standardised benches |

Full reference at [docs.rs/subms](https://docs.rs/subms).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
