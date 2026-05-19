#!/usr/bin/env bash
# Per-job step summary for the publish-rust job. Writes a markdown rollup
# (crates.io + docs.rs links) to $GITHUB_STEP_SUMMARY.
#
# Required env:
#   STATUS - job.status (success | failure | cancelled)
#   VER    - the version being published (no leading v)

set -euo pipefail

: "${STATUS:?STATUS not set}"
: "${VER:?VER not set}"
: "${GITHUB_STEP_SUMMARY:?must run under GitHub Actions}"

{
  echo "## Rust publish"
  echo ""
  if [ "$STATUS" = "success" ]; then
    echo ":white_check_mark: \`subms ${VER}\` published to crates.io"
    echo ""
    echo "- [View on crates.io](https://crates.io/crates/subms/${VER})"
    echo "- [Docs on docs.rs](https://docs.rs/subms/${VER})"
  else
    echo ":x: cargo publish failed - review logs above"
  fi
} >> "$GITHUB_STEP_SUMMARY"
