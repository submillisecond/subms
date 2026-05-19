#!/usr/bin/env bash
# Cross-checks that the pushed tag matches the version in both rust/Cargo.toml
# and java/pom.xml. Writes `version` + `is_prerelease` to $GITHUB_OUTPUT so
# downstream jobs can fan out from the same source of truth.
#
# Fails the job if any of the three (tag, Cargo.toml, pom.xml) disagree.
#
# Required env (provided by Actions):
#   GITHUB_REF    - refs/tags/v<x>.<y>.<z>[-rc]
#   GITHUB_OUTPUT - output-fact destination

set -euo pipefail

TAG="${GITHUB_REF#refs/tags/}"
VER="${TAG#v}"

echo "version=$VER" >> "$GITHUB_OUTPUT"

if echo "$VER" | grep -q -- '-'; then
  echo "is_prerelease=true" >> "$GITHUB_OUTPUT"
else
  echo "is_prerelease=false" >> "$GITHUB_OUTPUT"
fi

CARGO_VER=$(grep -E '^version\s*=' rust/Cargo.toml | head -1 | sed -E 's/.*"(.*)".*/\1/')
POM_VER=$(grep -m1 '<version>' java/pom.xml | sed -E 's/.*<version>(.*)<\/version>.*/\1/')

echo "tag=$VER cargo=$CARGO_VER pom=$POM_VER"

if [ "$CARGO_VER" != "$VER" ] || [ "$POM_VER" != "$VER" ]; then
  echo "::error::Tag v$VER does not match Cargo.toml ($CARGO_VER) or pom.xml ($POM_VER). Bump manifests before tagging."
  exit 1
fi
