# Changelog

All notable changes to `subms` (the sub-millisecond perf harness) are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Every Java harness stamps the JVM facts that change what a number MEANS.**
  `SubMsPerfHarness` now records `java_version`, `jvm_vm` and `jvm_heap_max` into
  `meta` at construction. Stamped by the harness rather than by each recipe,
  because it is identical for every capture and a recipe that forgets produces a
  number nobody can situate. A recipe setting the same key afterwards still wins.

  `jvm_heap_max` matters more than it looks. Host RAM tells you about the box,
  not about the run: the JVM takes a fraction of it, the same bench under a
  256 MB and a 700 MB ceiling are different experiments, and a GC-bound tail does
  not announce itself in the percentiles that come out. A capture that cannot say
  which ceiling it ran under cannot be compared with one that can.

## [0.9.0] - 2026-08-04

### Added

- **`SubMsFeatureCategory::Reported` / `REPORTED` - flat and per-op, but above the
  1 ms claim line.** `classify_feature` had NO upper bound: any flat measurement
  more than 10% above base returned `HotPath`, whether it read 65ns or 39 ms. A
  cookbook recipe was publishing a 30.7 ms operation as a per-op sub-millisecond
  claim. Distinct from `Structural`, which means O(n) - an op can be genuinely
  size-independent and still cost 30 ms, and reusing `Structural` for it would
  make that variant's own definition false.

- **`SubMsFeatureCategory::Indeterminate` / `INDETERMINATE` - the measurement
  cannot separate the feature from the guard.** Both tests now decline inside a
  band rather than picking a side. Measured across 112 classified features: 30
  (27%) sat within 1.35x of the guard that decided them, two flipped between
  consecutive runs of UNCHANGED code on the same box (one on a 3 ns move), and
  two disagreed across the Rust and Java ports of the same algorithm. Every one
  of those four was the base-delta test; no scaling verdict flipped or split.

  The base-delta band is expressed on the EXCESS over base (5%..15%), not on the
  guard. Banding the guard - the obvious first cut - swallowed a feature
  measuring exactly base, which is the least ambiguous auxiliary there is.

  It narrows the problem rather than removing it: one observed flip came from the
  BASE moving 36% while the feature moved 0.2%, and no band around a moving base
  is stable. Documented at the constant rather than implied away.

- **Per-feature provenance.** `set_feature` / `setFeature` now stamps each feature
  with the provenance of the run that wrote it, and `feature_p99_source` /
  `featureP99Source` reads it, falling back to the file-level stamp for older
  manifests. The manifest MERGE-writes, so a feature the current run did not
  measure keeps its previous numbers - and under a file-level stamp alone it
  silently inherited the new one. A laptop-measured feature was sitting inside a
  file marked `fleet`. A local re-run also clears a stale fleet reference rather
  than leaving an instance id attached to laptop numbers.

### Changed

- `classify_feature` / `classify` may now return either new category. A consumer
  matching exhaustively on the three previous values must handle both; the wire
  values are `reported` and `indeterminate`.

## [0.8.2] - 2026-08-03

### Added

- **`SubMsP99Source` - provenance for the figures in a feature manifest.**
  A latency number describes the machine that produced it, and the feature bench
  runs wherever it is invoked, so a manifest carrying `p99ByStage` without saying
  where it came from is indistinguishable from a conformance-box capture. New
  enum (`Local` / `Fleet`, wire tokens `local` / `fleet`) plus
  `SubMsFeatureManifest::set_p99_source` / `p99_source` / `p99_source_ref`
  (Rust) and `setP99Source` / `p99Source` / `p99SourceRef` (Java). An unstamped
  manifest reads as `Local` and a token that does not parse reads as `Local`
  too - both fail toward withholding numbers rather than publishing
  unattributed ones. A local re-stamp CLEARS a stale fleet reference, so a
  manifest cannot keep claiming a box it was last measured on; an empty
  instance id is treated as absent, since it would look like provenance while
  identifying nothing.
- **`SubMsP99Source::from_env()` (Rust) / `fromEnv()` + `instanceFromEnv()`
  (Java).** Reads `SUBMS_FLEET_INSTANCE`: present and non-blank means a fleet
  capture on that EC2 instance, absent means local. The env var is the contract
  between the fleet orchestrator and every recipe's `perf_features` target, so
  no recipe hand-rolls its own detection and a run anywhere else is Local by
  omission rather than by remembering to say so.

### Fixed

- **`classify_feature` claimed a feature was "within 10% of base" when it was
  far below it.** The auxiliary branch fires for anything at or under the
  baseline, so a feature measuring 200ns against a 700ns base was recorded as
  "flat p99 200ns within 10% of base 700ns". The CATEGORY was right - no cost on
  the hot path either way - but `perfReason` is the only audit trail the
  decision has, and that one was simply false. A figure below base now reads
  "at or below base Xns - no hot-path cost"; the within-the-band wording is
  reserved for a figure that is genuinely inside it. Both ports, 2 tests each.
- **`set_p99_source` with an empty fleet reference no longer records it.** The
  Rust match arm accepted any `Some(_)`, so `Some("")` wrote an empty
  `p99_source_ref` - a stamp that satisfies a presence check while naming no
  box. Now guarded, matching the Java port.

### Notes

- 10 new tests per port (Rust 148 total, Java 160), covering the unstamped
  default, the fleet stamp, the stale-reference clear, the ignored-on-local
  reference, the empty id, a `load_str` round trip preserving other fields, an
  unknown wire token, key position on re-stamp, and env derivation.
- Consumers of the feature manifest must publish `p99ByStage` only when
  `p99_source` is `fleet`. The site renderer already gates on this.

## [0.8.1] - 2026-07-30

### Added

- **Per-recipe bench configuration** (`bench_config` module): `SubMsBenchConfig`
  is the typed load/merge-save model of a recipe's `.subms/perf/controls.json`.
  Like `SubMsFeatureManifest` it round-trips through a zero-dependency JSON value
  model that PRESERVES every field the harness does not own (the fleet
  orchestrator's `sample_cap`/`rounds`, a third party's custom keys); a setter
  touches only the key it names. Typed accessors: `cpu_pin` (a `SubMsCpuPin` enum
  - `Single` = pin to one isolated core, the absent default for a single-threaded
  recipe; `Multi` = pin across `cores` cores; `None` = unpinned across all cores;
  a multi-threaded recipe uses `Multi`/`None` so a single-core pin does not starve
  its worker thread; the legacy boolean form still reads), `cores`, `sample_cap`,
  `reason`. New public surface: `SubMsBenchConfig`, `SubMsCpuPin`.

## [0.8.0] - 2026-07-29

### Added

- **Per-feature latency classification + manifest** (`feature` module): the harness
  now *decides* a library feature's latency category from a size sweep rather than
  taking a hand-authored label. `classify_feature(sweep, base_p99, override)`
  returns `hot-path` (flat/sub-linear per-op p99, above base), `structural`
  (p99 scales ~linearly with size - O(n)), or `auxiliary` (no workload, or a p99
  within noise of the base - a measured non-effect), with a recorded `override`
  escape hatch for the ambiguous cases. `SubMsFeatureManifest` loads/merge-saves a
  per-language `.subms/features/<lang>.json` through a hand-written, zero-dependency
  JSON value model that PRESERVES every field the harness does not own (a
  third party's custom keys survive round-trips); it touches only a feature's
  `perf` rating + `p99ByStage`. New public surface: `SubMsFeatureCategory`,
  `SubMsFeatureManifest`, `classify_feature`, `Json`, `parse_json`.

## [0.7.1] - 2026-07-28

Initial release. `0.7.1` is the baseline version for `subms`; all earlier
pre-release history is retired. `0.7.0` was published then yanked to correct the
crate `homepage` metadata (now the perf-harness cookbook primer, not the github
repo); `0.7.1` is the first intended public version on this line.
