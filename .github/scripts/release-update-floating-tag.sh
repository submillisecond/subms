#!/usr/bin/env bash
# Moves the floating major tag (`v0`, `v1`, ...) to the just-released SHA so
# consumers tracking `@v0` get the latest patch automatically.
#
# No-op for tags that don't match the v<major>.<minor>.<patch> shape (e.g.
# pre-release tags like v0.3.0-rc1 - the workflow gates this step on
# is_prerelease == 'false' anyway, but the check here is belt-and-braces).
#
# Required env (provided by Actions):
#   GITHUB_REF - refs/tags/v<x>.<y>.<z>

set -euo pipefail

TAG="${GITHUB_REF#refs/tags/}"
MAJOR=$(echo "$TAG" | sed -E 's/^v([0-9]+).*/v\1/')

if [ "$MAJOR" = "$TAG" ]; then
  echo "tag $TAG doesn't match v<major>.<minor>.<patch>; skipping floater"
  exit 0
fi

git config user.name  "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git tag -f "$MAJOR" "$TAG"
git push origin "$MAJOR" --force
