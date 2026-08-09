#!/usr/bin/env bash
# Release version guard (M8 Task 1): a pushed tag `vX.Y.Z` must exactly match
# [workspace.package].version in the root Cargo.toml. Run in release.yml
# before (and again just before) creating the GitHub Release, so a forgotten
# version bump fails loudly instead of shipping mislabeled artifacts.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  echo "Usage: $0 <tag>" >&2
  echo "  <tag> must look like vX.Y.Z and match [workspace.package].version in Cargo.toml" >&2
  exit 1
}

[ $# -eq 1 ] || usage
TAG="$1"

CARGO_VERSION="$("$SCRIPT_DIR/workspace-version.sh")"
EXPECTED_TAG="v${CARGO_VERSION}"

if [ "$TAG" != "$EXPECTED_TAG" ]; then
  {
    echo "ERROR: tag '$TAG' does not match workspace Cargo.toml version '$CARGO_VERSION'."
    echo "  expected tag: $EXPECTED_TAG"
    echo "  got tag:      $TAG"
    echo "Bump [workspace.package].version in Cargo.toml and re-tag before releasing."
  } >&2
  exit 1
fi

echo "OK: tag '$TAG' matches workspace version '$CARGO_VERSION'."
