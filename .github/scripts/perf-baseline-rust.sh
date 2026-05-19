#!/usr/bin/env bash
# Builds + runs the Rust self-bench at the PR's base SHA, captures the JSON
# to ../rust-baseline.json (relative to the working directory).
#
# Uses a separate git worktree under ../base-tree so the PR's tree stays
# untouched. Sets `exists=true` on the step's output to signal that a
# baseline was produced (downstream diff job gates on it).
#
# Required env:
#   BASE_SHA      - PR base SHA (provided via workflow env)
#   GITHUB_OUTPUT - step output destination

set -euo pipefail

: "${BASE_SHA:?BASE_SHA not set}"
: "${GITHUB_OUTPUT:?must run under GitHub Actions}"

git worktree add ../base-tree "$BASE_SHA"
( cd ../base-tree/rust && cargo build --release --example perf_main )
../base-tree/rust/target/release/examples/perf_main > rust-baseline.json

echo "exists=true" >> "$GITHUB_OUTPUT"
