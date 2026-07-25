# Changelog

All notable changes to `subms` are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.2] - 2026-07-27

### Added
- `SubMsStage.warmThenTime(warmup, measured, op)` (Java, two overloads:
  `Runnable` and `IntConsumer`): the Java mirror of the Rust
  `warm_then_time` shipped in 0.5.1. Drives HotSpot to C2 (and lets
  escape analysis elide short-lived allocations) before measuring, so a
  JIT-cold low-iteration stage no longer reads orders of magnitude slow.
- `SubMsObserver` trait (Rust) / interface (Java) + `ObservationCtx`
  (Java: `SubMsObservationCtx` record) + `SubMsStageKind` enum
  (`HotPath` / `BatchOp` / `OneShot` / `Unspecified`). Register an
  observer on `SubMsPerfHarness` via `with_observer(...)` / `setObserver(...)`;
  it receives every recorded sample (via `Stage::record`, `time`,
  `warm_then_time`, and `SubMsPacedStage::time`) plus the post-bench summary
  emitted by `summarize`. Default no-op trait/interface, so the hook costs
  nothing when no observer is registered. Stages now also carry an optional
  `SubMsStageKind` annotation (`stage.with_kind(...)` / `stage.withKind(...)`)
  so observers can choose histogram bucket boundaries that fit. The hook
  is what sibling adapters like `subms-otel` plug into - the harness itself
  stays zero-dep.

### Note
- Java side jumps from 0.5.0 to 0.5.2 directly; the 0.5.1 cut was published
  to crates.io only (the Maven staging step was skipped). The 0.5.2 jar
  carries both the warmup primitive and the observer hook for Java
  consumers, so no behaviour is missed by skipping 0.5.1 on Maven.

## [0.5.1] - 2026-07-27

Published to crates.io only (no Maven Central release). Java consumers
should jump from 0.5.0 straight to 0.5.2, which folds in both changes.

### Added
- `SubMsStage::warm_then_time(warmup, measured, op)` (Rust): run the
  operation `warmup` untimed iterations before recording `measured` timed
  samples. Primes caches and the branch predictor before measurement so a
  cold low-iteration stage no longer reads orders of magnitude slow.

## [0.3.0] - 2026-07-27

Extracted from the [submillisecond cookbook monorepo](https://github.com/submillisecond/submillisecond.com)
into a standalone repository: [`github.com/submillisecond/subms`](https://github.com/submillisecond/subms).

The library is the same code that was published as `subms 0.2.x` from the
monorepo - this release is a fresh starting point with a clean repo and
governance scaffold, not a behavioural change.

### Added
- Standalone repository with full governance: README, MIT + Apache-2.0 dual
  licence, SECURITY.md, CODE_OF_CONDUCT.md, CONTRIBUTING.md, .gitignore.
- `ci.yml` workflow: cargo test + cargo clippy + mvn test on every push + PR.
- `release.yml` workflow: tag push -> crates.io publish + Maven Central deploy.
- `docs/JSON-CONTRACT.md`: full spec of the JSON shape the harness emits.
- README opening explains the `subms` name (sub-millisecond).

### Changed
- Version bumped from `0.2.2-rc1` to `0.3.0` to mark the repo extraction.
- README rewritten to lead with the library (not the cookbook context).
- Java side now dual-licensed (MIT + Apache-2.0) to match the Rust side.

### Removed
- `parse-perf.py` cookbook-specific helper (lived under `java/`; not relevant
  to the library).

## [0.2.x] (in the cookbook monorepo)

Earlier 0.2.x releases were published from the
[submillisecond/cookbook monorepo](https://github.com/submillisecond/submillisecond.com).
See the original repo's history for those notes. The 0.3.0 line continues
the version trajectory from a fresh repo.
