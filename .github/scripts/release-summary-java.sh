#!/usr/bin/env bash
# Per-job step summary for the publish-java job. Maven Central staging
# requires a manual Publish click on the portal; this surfaces the link
# directly in the run page so the operator doesn't have to hunt for it.
#
# Required env:
#   STATUS - job.status
#   VER    - the version being published

set -euo pipefail

: "${STATUS:?STATUS not set}"
: "${VER:?VER not set}"
: "${GITHUB_STEP_SUMMARY:?must run under GitHub Actions}"

{
  echo "## Java publish (staged)"
  echo ""
  if [ "$STATUS" = "success" ]; then
    echo ":white_check_mark: \`com.submillisecond:subms:${VER}\` uploaded to Sonatype Central as a **staged deployment**"
    echo ""
    echo "**Manual step required:** log in to the portal and release the staged artifact."
    echo ""
    echo "- [Sonatype Central deployments dashboard](https://central.sonatype.com/publishing) - click **Publish** on the staged \`com.submillisecond:subms:${VER}\` deployment to ship it. Click **Drop** to roll back instead."
    echo "- [Maven Central artifact page](https://central.sonatype.com/artifact/com.submillisecond/subms/${VER}) - 200 once the staged deployment is published."
  else
    echo ":x: mvn deploy failed - review logs above. The staged deployment (if any) can be dropped from the [Central deployments dashboard](https://central.sonatype.com/publishing)."
  fi
} >> "$GITHUB_STEP_SUMMARY"
