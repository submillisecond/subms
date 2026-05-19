#!/usr/bin/env bash
# Final-job step summary: rolls up the release into a single panel with
# next-step links (Maven portal, cookbook re-run reminder, consumer pin
# advice).
#
# Required env:
#   STATUS            - job.status
#   VER               - released version (no leading v)
#   PRE               - 'true' if this is a pre-release
#   GITHUB_REPOSITORY - owner/repo (provided by Actions)

set -euo pipefail

: "${STATUS:?STATUS not set}"
: "${VER:?VER not set}"
: "${PRE:?PRE not set}"
: "${GITHUB_REPOSITORY:?must run under GitHub Actions}"
: "${GITHUB_STEP_SUMMARY:?must run under GitHub Actions}"

MAJOR="v$(echo "${VER}" | cut -d. -f1)"

{
  echo "## Release v${VER}"
  echo ""
  if [ "$STATUS" = "success" ]; then
    echo ":white_check_mark: GitHub Release created with auto-generated notes"
    if [ "$PRE" = "false" ]; then
      echo ":white_check_mark: Floating major tag \`${MAJOR}\` moved to this SHA - consumers tracking \`@${MAJOR}\` are now on \`v${VER}\`"
    else
      echo ":information_source: Pre-release; floating major tag not moved"
    fi
    echo ""
    echo "- [Release notes](https://github.com/${GITHUB_REPOSITORY}/releases/tag/v${VER})"
    echo ""
    echo "### Next steps"
    echo ""
    echo "1. **Publish the staged Maven deployment** at <https://central.sonatype.com/publishing>."
    echo "2. **Re-run subms-cookbook CI** once both registries serve \`${VER}\` - the cookbook recipe deps will then resolve."
    echo "3. **Update consumers** to pin \`subms = \"${VER}\"\` / \`<version>${VER}</version>\` (or float on \`${MAJOR}\` for auto-patches)."
  else
    echo ":x: GitHub Release step failed - review logs above"
  fi
} >> "$GITHUB_STEP_SUMMARY"
