#!/usr/bin/env bash
# Builds + runs the Java self-bench at the PR's base SHA, captures the JSON
# to ../java-baseline.json. Mirror of perf-baseline-rust.sh.
#
# IMPORTANT: relative paths are interpreted against the calling step's
# working-directory (set to `java` in the workflow), so the worktree lands
# two levels up (.../base-tree).
#
# Required env:
#   BASE_SHA      - PR base SHA
#   GITHUB_OUTPUT - step output destination

set -euo pipefail

: "${BASE_SHA:?BASE_SHA not set}"
: "${GITHUB_OUTPUT:?must run under GitHub Actions}"

git worktree add ../../base-tree "$BASE_SHA"
( cd ../../base-tree/java && mvn -q -DskipTests test-compile )
java -cp "../../base-tree/java/target/test-classes:../../base-tree/java/target/classes" \
     com.submillisecond.perf.bench.PerfMain > ../java-baseline.json

echo "exists=true" >> "$GITHUB_OUTPUT"
