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
subms = "0.3"
```

### Java (Maven)

```xml
<dependency>
    <groupId>com.submillisecond</groupId>
    <artifactId>subms</artifactId>
    <version>0.3.0</version>
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

- **v0.x.y**: API may change between minors. Pin to a precise tag (`subms = "=0.3.0"`) for stability before v1.
- **v1.0.0**: API frozen; semver thereafter.
- **JSON contract**: stable since v0.2. Field renames will require a major bump.

## License

Dual-licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

at your option. The MIT-only fallback is also at [`LICENSE`](LICENSE) for tooling that expects a single LICENSE file.

## Contributing

PRs welcome. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the dev flow + the "Rust + Java parity" rule (changing one side requires changing the other).

## Dogfooding

This repo runs its own ecosystem as a working demo. Every PR fires the full
chain:

1. **`rust/examples/perf_main.rs`** and **`java/.../bench/PerfMain.java`** run a self-bench that exercises the harness's hot paths (`stage.time(...)`, `summarize`, `summary_to_json`, `diff_summary`).
2. **[`subms-action-bench`](https://github.com/submillisecond/subms-action-bench)** captures the candidate JSON; the base ref's JSON is rebuilt from a git worktree.
3. **[`subms-action-diff`](https://github.com/submillisecond/subms-action-diff)** diffs candidate vs baseline per language; produces a `subms-diff.json` artifact per matrix entry.
4. **[`subms-action-diff-sink`](https://github.com/submillisecond/subms-action-diff-sink)** pushes the diff to stdout (visible in the workflow log) and a `*-perf-feed.jsonl` file (uploaded as an artifact for trend tracking).
5. **[`subms-action-diff-aggregate`](https://github.com/submillisecond/subms-action-diff-aggregate)** rolls the Rust + Java diffs into **one** sticky PR comment with the top regressions across the matrix.

If the harness's own hot-path overhead regresses beyond the per-stage threshold, the gate surfaces it. The same `.github/workflows/perf.yml` shape is what downstream consumers set up to gate their own perf-critical PRs.

Plus a **pre-commit hook** at [`.pre-commit-config.yaml`](.pre-commit-config.yaml) catches regressions before they leave a contributor's laptop — same diff math, run locally on every commit that touches a `perf.json`.

## Status

**v0.3.0**, pre-1.0. Used internally across the [submillisecond cookbook](https://submillisecond.com/cookbook) (16 dual-language recipes). Not yet hardened by external adoption — early bug reports help.
