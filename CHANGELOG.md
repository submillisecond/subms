# Changelog

All notable changes to `subms` (the sub-millisecond perf harness) are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- The jar manifest carries `Implementation-Version`, so `SubMsPerfHarness` reads the harness version from the artifact rather than always falling back to a hand-maintained constant. Shipped 0.9.3 without it, which left the manifest path dead code and the constant the only source - it works, and the pom-vs-constant test guards it, but a version nobody has to remember to bump is the better answer.

## [0.9.3] - 2026-08-06

### Added

- Every capture stamps `harness_version` into `meta`, on both ports. The harness is the instrument, and a measurement that does not name its instrument is reconstructible only by luck - the same recipe run under two harness releases is two experiments.
- Rust reads `env!("CARGO_PKG_VERSION")`, so it cannot drift from the crate it shipped in.
- Java reads the jar manifest, falling back to a constant for classes-directory runs, which is how the benches execute under `mvn exec:java`. The constant is the drift risk, so a test reads `pom.xml` and fails the build when the two disagree rather than letting a wrong version reach every published capture.
- Prompted by a reader asking whether a published number carries enough provenance to be rebuilt. The bench orchestrator already recorded the box, the commit and the compiler; the instrument itself was the gap.

## [0.9.2] - 2026-08-05

### Added

- The storage-growth harness now exists in Java (`SubMsGrowth`, `SubMsGrowthRecipe`), emitting the same byte-equivalent JSON as the Rust module. Until now a curve could only come from the Rust port.
- The footprint is supplied by the recipe. `diskBytes` / `memoryBytes` / `liveBytes` ask the structure what it holds; the harness never inspects the heap, so a Java curve is a measurement rather than a GC reading.
- Both suites pin the same exact JSON fixture, latencies zeroed so the encoder is the only variable.
- The fixture includes an exact rounding tie, 1568/1024 = 1.53125. Rust's `{:.4}` is half-to-even and `String.format` is half-up, so the Java encoder rounds through `BigDecimal` with `HALF_EVEN`.
- Rust is unchanged in this release beyond that fixture test. Both ports bump together so nobody has to work out which language is a version behind.

## [0.9.1] - 2026-08-04

### Added

- Every Java harness stamps `java_version`, `jvm_vm` and `jvm_heap_max` into `meta` at construction. A recipe setting the same key afterwards still wins.
- Stamped by the harness because it is identical for every capture, and a recipe that forgets produces a number nobody can situate.
- `jvm_heap_max` matters more than it looks. Host RAM describes the box, not the run: the JVM takes a fraction of it, and the same bench under a 256 MB and a 700 MB ceiling are different experiments.
- A GC-bound tail does not announce itself in the percentiles. A capture that cannot say which ceiling it ran under cannot be compared with one that can.

## [0.9.0] - 2026-08-04

### Added

- `SubMsFeatureCategory::Reported` / `REPORTED`: flat and per-op, but above the 1 ms claim line.
- `classify_feature` had no upper bound. Any flat measurement more than 10% above base returned `HotPath`, whether it read 65ns or 39 ms, and a cookbook recipe was publishing a 30.7 ms operation as a per-op sub-millisecond claim.
- `Structural` means O(n), so it could not absorb this. An op can be size-independent and still cost 30 ms; reusing the variant would make its own definition false.
- `SubMsFeatureCategory::Indeterminate` / `INDETERMINATE`: the measurement cannot separate the feature from the guard, so both tests now decline inside a band.
- Across 112 classified features, 30 sat within 1.35x of the guard that decided them. Two flipped between consecutive runs of unchanged code on the same box, one on a 3 ns move, and two disagreed across the Rust and Java ports of the same algorithm.
- All four were the base-delta test. No scaling verdict flipped or split.
- The base-delta band sits on the excess over base (5%..15%). Banding the guard was the obvious first cut and it swallowed a feature measuring exactly base, which is the least ambiguous auxiliary there is.
- It narrows the problem without removing it. One flip came from the base moving 36% while the feature moved 0.2%, and no band around a moving base is stable. Documented at the constant.
- Per-feature provenance: `set_feature` / `setFeature` stamps each feature with the provenance of the run that wrote it; `feature_p99_source` / `featureP99Source` reads it, falling back to the file-level stamp for older manifests.
- The manifest merge-writes, so a feature the current run did not measure keeps its old numbers. Under a file-level stamp alone it silently inherited the new one, and a laptop-measured feature was sitting inside a file marked `fleet`.
- A local re-run clears a stale fleet reference rather than leaving an instance id attached to laptop numbers.

### Changed

- `classify_feature` / `classify` may now return either new category. A consumer
  matching exhaustively on the three previous values must handle both; the wire
  values are `reported` and `indeterminate`.

## [0.8.2] - 2026-08-03

### Added

- `SubMsP99Source`: provenance for the figures in a feature manifest. `Local` / `Fleet`, wire tokens `local` / `fleet`.
- A latency number describes the machine that produced it, and the feature bench runs wherever it is invoked. A manifest carrying `p99ByStage` without saying where it came from is indistinguishable from a conformance-box capture.
- New surface: `SubMsFeatureManifest::set_p99_source` / `p99_source` / `p99_source_ref`, and `setP99Source` / `p99Source` / `p99SourceRef` in Java.
- An unstamped manifest reads `Local`, and so does a token that does not parse. Both fail toward withholding numbers.
- A local re-stamp clears a stale fleet reference, so a manifest cannot keep claiming a box it is no longer measured on. An empty instance id is treated as absent, since it looks like provenance while identifying nothing.
- `SubMsP99Source::from_env()` / `fromEnv()` + `instanceFromEnv()` read `SUBMS_FLEET_INSTANCE`. The env var is the contract between the fleet orchestrator and every recipe's `perf_features` target, so no recipe hand-rolls detection and a run anywhere else is Local by omission.

### Fixed

- `classify_feature` claimed a feature was "within 10% of base" when it was far below it. A feature measuring 200ns against a 700ns base was recorded as "flat p99 200ns within 10% of base 700ns".
- The category was right either way, but `perfReason` is the only audit trail the decision has and that one was false. A figure below base now reads "at or below base Xns - no hot-path cost". Both ports, 2 tests each.
- `set_p99_source` with an empty fleet reference no longer records it. The Rust match arm accepted any `Some(_)`, so `Some("")` wrote a stamp that satisfies a presence check while naming no box. Now guarded, matching Java.

### Notes

- 10 new tests per port (Rust 148, Java 160): the unstamped default, the fleet stamp, the stale-reference clear, the ignored-on-local reference, the empty id, a `load_str` round trip preserving other fields, an unknown wire token, key position on re-stamp, and env derivation.
- Publish `p99ByStage` only when `p99_source` is `fleet`. The site renderer already gates on this.

## [0.8.1] - 2026-07-30

### Added

- `SubMsBenchConfig` (`bench_config`): typed load and merge-save for a recipe's `.subms/perf/controls.json`.
- Round-trips through the same zero-dependency JSON value model as `SubMsFeatureManifest`, preserving every field the harness does not own. A setter touches only the key it names.
- Typed accessors: `cpu_pin`, `cores`, `sample_cap`, `reason`.
- `SubMsCpuPin` is `Single` (one isolated core, the absent default for a single-threaded recipe), `Multi` (pinned across `cores`), or `None` (unpinned). A multi-threaded recipe takes `Multi` or `None` so a single-core pin does not starve its worker thread. The legacy boolean form still reads.
- New public surface: `SubMsBenchConfig`, `SubMsCpuPin`.

## [0.8.0] - 2026-07-29

### Added

- The harness now decides a feature's latency category from a size sweep instead of taking a hand-authored label (`feature` module).
- `classify_feature(sweep, base_p99, override)` returns `hot-path` (flat or sub-linear per-op p99, above base), `structural` (p99 scales linearly with size), or `auxiliary` (no workload, or a p99 within noise of base). A recorded `override` covers the ambiguous cases.
- `SubMsFeatureManifest` loads and merge-saves `.subms/features/<lang>.json` through a hand-written zero-dependency JSON value model. A third party's custom keys survive the round trip; the harness touches only `perf` and `p99ByStage`.
- New public surface: `SubMsFeatureCategory`, `SubMsFeatureManifest`, `classify_feature`, `Json`, `parse_json`.

## [0.7.1] - 2026-07-28

Initial release. `0.7.1` is the baseline version for `subms`; all earlier
pre-release history is retired. `0.7.0` was published then yanked to correct the
crate `homepage` metadata (now the perf-harness cookbook primer, not the github
repo); `0.7.1` is the first intended public version on this line.
