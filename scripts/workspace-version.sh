#!/usr/bin/env bash
# Prints the axilog workspace version from [workspace.package].version in the
# root Cargo.toml. Single source of truth for the release pipeline (M8):
# scripts/check-tag-version.sh and .github/workflows/release.yml both shell
# out to this instead of re-parsing Cargo.toml themselves.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="${REPO_ROOT}/Cargo.toml"

if [ ! -f "$CARGO_TOML" ]; then
  echo "error: $CARGO_TOML not found" >&2
  exit 1
fi

VERSION="$(awk '
  /^\[workspace\.package\]/ { in_section = 1; next }
  /^\[/ { in_section = 0 }
  in_section && /^version[[:space:]]*=/ {
    match($0, /"[^"]*"/)
    v = substr($0, RSTART + 1, RLENGTH - 2)
    print v
    exit
  }
' "$CARGO_TOML")"

if [ -z "$VERSION" ]; then
  echo "error: could not find [workspace.package].version in $CARGO_TOML" >&2
  exit 1
fi

echo "$VERSION"
