# Changelog

All notable changes to `subms` (the sub-millisecond perf harness) are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
