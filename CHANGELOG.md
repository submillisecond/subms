# Changelog

All notable changes to `subms` are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-05-19

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
