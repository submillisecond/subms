# Contributing to subms

Thanks for considering a contribution. `subms` is a small, focused library;
PRs that keep it small and focused are welcome.

## The parity rule

`subms` ships **two** language surfaces - Rust and Java - that must stay
behavioural-equivalent. The JSON contract is byte-identical across them, and
the API names map 1:1 modulo case style.

**Rule:** if you change one side, you change the other in the same PR.

Examples:

- Adding a new sink type (e.g. `with_jitter`) -> add to both `lib.rs` and the
  Java `SubMsPerfHarness` class. Add tests on both sides.
- Renaming a public type (e.g. `SubMsBenchSummary`) -> rename the Java record
  too. Update both READMEs. Update the JSON serialiser only if the field
  names changed (they shouldn't, normally).
- Bumping the JSON shape (rare) -> requires a major version bump.

PRs that only touch one language will be asked for the other side before
merge. If you genuinely can only implement one side, open the PR with a clear
note - someone will help port it.

## Quick rules

- Open an issue first for non-trivial changes so we can align on shape.
- One concern per PR; review remains tractable.
- ASCII-only in source + docs. No em-dash / en-dash / curly quotes.
- New behaviour comes with tests (Rust: `cargo test`; Java: `mvn test`).
- New public API comes with docstrings + a README example.

## Local development

### Rust

```bash
cd rust
cargo build
cargo test
cargo test --doc                    # doctests must pass too
cargo fmt --check
cargo clippy -- -D warnings
```

### Java

```bash
cd java
mvn -q test
```

The full local CI pre-flight:

```bash
( cd rust && cargo test && cargo fmt --check && cargo clippy -- -D warnings ) && \
( cd java && mvn -q test )
```

## Release flow

`subms` follows semver. Cutting a release:

1. Update `CHANGELOG.md`.
2. Bump the version in **both** `rust/Cargo.toml` and `java/pom.xml`. They
   stay in lockstep.
3. Open a release PR. CI must be green.
4. Merge.
5. Tag: `git tag v<major>.<minor>.<patch>` and push the tag.
6. The release workflow publishes to crates.io and Maven Central.
7. After publish, move the floating major tag (`v0` -> the new SHA) to keep
   `submillisecond/subms@v0` consumers on the latest patch.

## Reporting bugs

Open a GitHub issue with:

- The version (crate / artifact) you're on
- A minimal reproduction (Rust or Java code that triggers the bug)
- What you expected vs what you got
- The full stack trace / panic output / JSON output if relevant

For security issues, see [SECURITY.md](SECURITY.md) - do not open a public
issue for vulnerabilities.

## Licence

By contributing, you agree your contributions are dual-licensed under the
[MIT License](LICENSE-MIT) and the [Apache License 2.0](LICENSE-APACHE), matching
the rest of the project.
