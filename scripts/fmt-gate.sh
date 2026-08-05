#!/usr/bin/env bash
# cargo fmt --check for this repo's Rust workspace, run as a pre-commit gate.
#
# FINDS cargo rather than trusting PATH: git GUIs launch hooks from a minimal
# environment that usually lacks ~/.cargo/bin, so relying on PATH alone means the
# desktop client silently never runs this gate. rustup always installs to
# $CARGO_HOME/bin (default ~/.cargo/bin), so the fallbacks cover every machine
# that has it - including Windows, where a GUI sets USERPROFILE but not HOME.
#
# Lives in a script rather than inline in .pre-commit-config.yaml because the
# messages contain ": ", which YAML parses as a mapping inside an unquoted
# scalar and rejects.
set -euo pipefail

CARGO="$(command -v cargo 2>/dev/null || true)"
if [ -z "$CARGO" ]; then
  for c in "${CARGO_HOME:-}/bin/cargo" "${HOME:-}/.cargo/bin/cargo" \
           "${USERPROFILE:-}/.cargo/bin/cargo" "${CARGO_HOME:-}/bin/cargo.exe" \
           "${HOME:-}/.cargo/bin/cargo.exe" "${USERPROFILE:-}/.cargo/bin/cargo.exe"; do
    if [ -x "$c" ]; then CARGO="$c"; break; fi
  done
fi
# Skipping is the last resort, and only when cargo is genuinely absent. Treating
# "could not run the check" as "check failed" blocks a commit on a measurement
# that never happened, which is how this gate first went wrong.
if [ -z "$CARGO" ]; then
  echo "fmt gate skipped, cargo not found on PATH or under CARGO_HOME/~/.cargo (CI still enforces it)" >&2
  exit 0
fi

exec "$CARGO" fmt --manifest-path rust/Cargo.toml "$@" --check
