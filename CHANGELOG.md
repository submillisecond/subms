# Changelog

All notable changes to `subms` (the sub-millisecond perf harness) are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
