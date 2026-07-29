# subms

**The sub-millisecond perf harness.** Zero-dependency Rust + Java library that
records timed samples per stage, computes percentiles, runs scale sweeps,
detects coordinated omission, and emits a stable JSON contract.

[![crates.io](https://img.shields.io/crates/v/subms.svg?logo=rust&style=flat-square)](https://crates.io/crates/subms)
[![maven central](https://img.shields.io/maven-central/v/com.submillisecond/subms.svg?logo=apache-maven&style=flat-square)](https://central.sonatype.com/artifact/com.submillisecond/subms)
[![ci](https://github.com/submillisecond/subms/actions/workflows/ci.yml/badge.svg)](https://github.com/submillisecond/subms/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT_OR_Apache--2.0-blue.svg?style=flat-square)](#license)
[![docs.rs](https://img.shields.io/badge/docs.rs-subms-blue?style=flat-square&logo=rust)](https://docs.rs/subms)

> **subms** = *sub-millisecond*. The brand promise compressed into the name.
> The harness defends p99 < 1 ms across Rust and Java with byte-equivalent
> JSON the rest of the [submillisecond](https://submillisecond.com)
> ecosystem consumes.

## What it does

| | |
|---|---|
| **Records** | Per-stage timed samples via `stage.time(closure)` or `stage.record(ns)`. |
| **Summarises** | Sorted percentiles (p50, p99, p99.9, max, mean) + downsampled chronological timeline. |
| **Asserts** | `assert_p99_under(...)` fails the test when any stage breaches its budget. |
| **Sweeps** | `run_sweep([params...])` captures the same workload at varying inputs (scale curves, feature toggles). |
| **Diffs** | `diff_summary(baseline, candidate)` produces a typed regression delta. |
| **Times** | `SubMsTimer` autostart stopwatch with named checkpoints for mid-app instrumentation. |
| **Corrects** | `stage.with_pacing(rate)` folds queue delay into per-op latency (coordinated-omission backfill). |
| **Emits** | Stable JSON shape, byte-equivalent across Rust and Java. Consumed by [subms-action-*](https://github.com/submillisecond) for PR-time CI gates. |

## Install

### Rust

```toml
[dependencies]
subms = "0.5"
```

### Java (Maven)

```xml
<dependency>
    <groupId>com.submillisecond</groupId>
    <artifactId>subms</artifactId>
    <version>0.5.2</version>
</dependency>
```

JDK 21 baseline. Both surfaces are pure std-lib / pure JDK — no transitive dependencies.

## Quickstart

### Rust

```rust
use subms::{SubMsPerfHarness, summarize, assert_p99_under, SubMsBenchAssertion, print_summary};

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

### Java

```java
import com.submillisecond.perf.*;

SubMsPerfHarness h = new SubMsPerfHarness("my-workload", "java");
h.input("entries", "50000");

SubMsPerfHarness.Stage put = h.stage("put", 50_000);
for (int i = 0; i < 50_000; i++) {
    put.time(() -> { /* hot-path work under test */ });
}

SubMsBenchSummary summary = SubMsBench.summarize(h);
SubMsBench.printSummary(summary, System.out);

SubMsBench.assertP99Under(summary, List.of(
        new SubMsBench.Assertion("put", 1_000_000L)));
```

Identical surface, byte-equivalent JSON output.

## The JSON contract

```jsonc
{
  "workload":  "my-workload",
  "lang":      "rust",
  "timestamp": "2026-05-19T13:11:58Z",
  "inputs":    { "entries": "50000" },
  "meta":      { "host":    "ci-1" },
  "stages": {
    "put": {
      "count":      50000,
      "p50_ns":     300,
      "p99_ns":     1200,
      "p999_ns":    153900,
      "max_ns":     3895300,
      "mean_ns":    1761,
      "samples_ns": [...]
    }
  }
}
```

Full schema spec, diff variant, sweep variant, and adapter notes for JMH / Criterion / HdrHistogram in [`docs/JSON-CONTRACT.md`](docs/JSON-CONTRACT.md).

## Per-feature latency: the bench decides the category

A library's optional features are not all the same kind of thing. Some change the
per-op hot path (a real p99 claim). Some are O(n) whole-structure operations -
serialize, compaction - that cannot honestly carry a per-op sub-millisecond
number. Some are pure capabilities with no latency delta at all (a serde derive,
a debug counter). The harness measures each and lets the **bench decide** which is
which, from a size sweep - the taxonomy is an *output*, not a hand-authored label:

| category | how it is decided | meaning |
|---|---|---|
| `hot-path` | p99 stays flat / sub-linear as the structure grows, and sits above the base op | a measured per-op p99 claim |
| `structural` | p99 grows with size - O(n) or worse (the `STRUCTURAL_FRACTION` slope test) | O(n)+, excluded from the per-op claim |
| `auxiliary` | no workload registered, or a p99 within noise of the base op | a capability, no latency claim |

```rust
use subms::{classify_feature, SubMsFeatureCategory, SubMsFeatureManifest};
use std::collections::BTreeMap;

// A size sweep of (structure_size, p99_ns) for the feature's workload.
let sweep = [(1_024usize, 350u64), (16_384, 360), (262_144, 372)];
let (category, reason) = classify_feature(&sweep, /* base p99 */ Some(300), /* override */ None);
assert_eq!(category, SubMsFeatureCategory::HotPath);

// Merge the decision into the per-language manifest, preserving every field the
// harness does not own (yours, or another tool's).
let mut manifest = SubMsFeatureManifest::load_str("rust", existing_json);
let mut p99 = BTreeMap::new();
p99.insert("add".to_string(), 372);
manifest.set_feature("counting", category, &p99, &reason);
std::fs::write(".subms/features/rust.json", manifest.to_json())?;
```

`SubMsFeatureManifest` round-trips through a hand-written, zero-dependency JSON
value model, so a load/update/save touches **only** the target feature's `perf`
rating + `p99ByStage` - a third party can enrich the file with arbitrary fields
and they survive every harness write. An `override` escape hatch (recorded, with a
reason) handles the genuinely ambiguous cases (amortized, borderline O(log n)).

### The `.subms/` layout

Everything a `subms` harness emits for a project lives under one directory next to
the code - the same for a library benchmarking itself and for any downstream
consumer:

```text
<project>/.subms/
  perf/       <lang>.json + .raw.json + .storage.json   # official captures - git-tracked
  features/   <lang>.json                                # the feature manifest above
  local/      ...                                        # local dev captures - git-ignored
```

## Beyond a single bench: the ecosystem

The harness is most useful when paired with the CI / observability tooling that consumes its JSON shape:

| component | repo | does |
|---|---|---|
| **subms-action-bench** | [submillisecond/subms-action-bench](https://github.com/submillisecond/subms-action-bench) | Runs a bench command, captures JSON, retries on flake. |
| **subms-action-diff** | [submillisecond/subms-action-diff](https://github.com/submillisecond/subms-action-diff) | The PR-time regression gate. Sticky comment + status check + per-stage thresholds. |
| **subms-action-diff-aggregate** | [submillisecond/subms-action-diff-aggregate](https://github.com/submillisecond/subms-action-diff-aggregate) | Rolls N matrix diffs into one verdict. |
| **subms-action-diff-sink** | [submillisecond/subms-action-diff-sink](https://github.com/submillisecond/subms-action-diff-sink) | 13 downstream sinks: Slack / Datadog / S3 / Prometheus / Splunk / etc. |
| **subms-action-drift** | [submillisecond/subms-action-drift](https://github.com/submillisecond/subms-action-drift) | Welford rolling-window drift detection. |
| **subms-actions** (umbrella) | [submillisecond/subms-actions](https://github.com/submillisecond/subms-actions) | Reusable workflow + pre-commit hook + suite docs. |

You can use `subms` standalone for in-process measurement and never touch the actions. You can use the actions against any tool's JSON (with a tiny adapter) and never touch `subms`. They compose — they don't depend on each other.

## When to use what

| you want | reach for |
|---|---|
| One-call timing of a code section | `SubMsTimer` (mid-app stopwatch) |
| Multi-sample percentile measurement of a workload | `SubMsPerfHarness` + `stage.time()` |
| Constant-arrival-rate latency measurement | `stage.with_pacing(target_qps)` (Gil-Tene CO correction) |
| Compare scale curves (varying entries / threads / etc.) | `run_sweep([params...])` |
| Regression analysis between two runs | `diff_summary(baseline, candidate)` |
| Production-grade tracing / spans | OpenTelemetry, `tracing` crate. **Not this library.** |

## Stability

- **v0.x.y**: API may change between minors. Pin to a precise tag (`subms = "=0.5.2"`) for stability before v1.
- **v1.0.0**: API frozen; semver thereafter.
- **JSON contract**: stable since v0.2. Field renames will require a major bump.

## License

Dual-licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

at your option. The MIT-only fallback is also at [`LICENSE`](LICENSE) for tooling that expects a single LICENSE file.

## Contributing

PRs welcome. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the dev flow + the "Rust + Java parity" rule (changing one side requires changing the other).

## Self-bench (the harness gating itself)

This repo runs its own ecosystem against itself - every PR exercises the
full chain so the same setup downstream consumers use is validated on
every change:

1. **`rust/examples/perf_main.rs`** and **`java/.../bench/PerfMain.java`** run a self-bench that exercises the harness's hot paths (`stage.time(...)`, `summarize`, `summary_to_json`, `diff_summary`).
2. **[`subms-action-bench`](https://github.com/submillisecond/subms-action-bench)** captures the candidate JSON; the base ref's JSON is rebuilt from a git worktree.
3. **[`subms-action-diff`](https://github.com/submillisecond/subms-action-diff)** diffs candidate vs baseline per language; produces a `subms-diff.json` artifact per matrix entry.
4. **[`subms-action-diff-sink`](https://github.com/submillisecond/subms-action-diff-sink)** pushes the diff to stdout (visible in the workflow log) and a `*-perf-feed.jsonl` file (uploaded as an artifact for trend tracking).
5. **[`subms-action-diff-aggregate`](https://github.com/submillisecond/subms-action-diff-aggregate)** rolls the Rust + Java diffs into **one** sticky PR comment with the top regressions across the matrix.

If the harness's own hot-path overhead regresses beyond the per-stage threshold, the gate surfaces it. The same `.github/workflows/perf.yml` shape is what downstream consumers set up to gate their own perf-critical PRs.

Plus a **pre-commit hook** at [`.pre-commit-config.yaml`](.pre-commit-config.yaml) catches regressions before they leave a contributor's laptop — same diff math, run locally on every commit that touches a `perf.json`.

## Status

Pre-1.0 (current version on the crates.io / Maven Central badges above): the API may still shift between minors. Used across the [submillisecond cookbook](https://submillisecond.com/cookbook); bug reports and API feedback welcome.
