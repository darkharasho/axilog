#!/usr/bin/env bash
# Version single-source enforcement (M8 Task 2). The workspace Cargo.toml's
# [workspace.package].version is the single source of truth; this script
# asserts every other place a version string is duplicated by hand agrees
# with it:
#   - crates/axilog-node/package.json            ("version")
#   - crates/axilog-node/npm/<platform>/package.json  ("version", one per
#     napi-rs platform package dir)
#   - crates/axilog-node/package.json's own optionalDependencies pins on
#     those same platform packages (a stale pin would make `npm install`
#     silently resolve nothing, or the wrong version, for a released main
#     package -- so it's checked here too even though it's not a standalone
#     package.json "version" field)
#   - crates/axilog-py/pyproject.toml            ([project] version)
#
# On crates/axilog-py/pyproject.toml: maturin *can* read the version
# straight from Cargo.toml if [project] declares `dynamic = ["version"]`
# (see https://www.maturin.rs/metadata -- "Add the version dynamically").
# This project's pyproject.toml does NOT do that: [project] has a plain,
# static `version = "0.1.0"` string (checked directly, see below), so
# maturin builds whatever is written there regardless of Cargo.toml. That
# means the Python package version is exactly as hand-maintained as the
# Node one, and needs the same guard -- there is no dynamic-version path to
# fall back to asserting against here.
#
# Run from anywhere; paths are resolved relative to the repo root. Exits
# non-zero and prints every mismatch (not just the first) if anything is
# out of sync.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CARGO_VERSION="$("$REPO_ROOT/scripts/workspace-version.sh")" || exit 1

FAIL=0

json_version() {
  # $1: path to a package.json-shaped file. Prints its top-level "version".
  node -e '
    const fs = require("fs");
    const pkg = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
    if (typeof pkg.version !== "string" || pkg.version.length === 0) {
      process.exit(1);
    }
    process.stdout.write(pkg.version);
  ' "$1"
}

json_optional_dep_version() {
  # $1: path to package.json. $2: dependency name. Prints the pinned
  # version string, or nothing (exit 1) if absent.
  node -e '
    const fs = require("fs");
    const pkg = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
    const deps = pkg.optionalDependencies || {};
    const v = deps[process.argv[2]];
    if (typeof v !== "string" || v.length === 0) {
      process.exit(1);
    }
    process.stdout.write(v);
  ' "$1" "$2"
}

toml_project_version() {
  # $1: path to a pyproject.toml. Prints [project]'s "version" value.
  awk '
    /^\[project\]/ { in_section = 1; next }
    /^\[/ { in_section = 0 }
    in_section && /^version[[:space:]]*=/ {
      match($0, /"[^"]*"/)
      print substr($0, RSTART + 1, RLENGTH - 2)
      exit
    }
  ' "$1"
}

check() {
  # $1: human label. $2: actual value. $3: source path (for the message).
  local label="$1" actual="$2" source="$3"
  if [ -z "$actual" ]; then
    echo "MISMATCH: $label -- could not read a version from $source" >&2
    FAIL=1
  elif [ "$actual" != "$CARGO_VERSION" ]; then
    echo "MISMATCH: $label -- $source has '$actual', workspace Cargo.toml has '$CARGO_VERSION'" >&2
    FAIL=1
  else
    echo "OK: $label ($actual)"
  fi
}

echo "workspace Cargo.toml [workspace.package].version = $CARGO_VERSION"
echo

# --- crates/axilog-node/package.json ---
NODE_PKG="$REPO_ROOT/crates/axilog-node/package.json"
check "crates/axilog-node/package.json" "$(json_version "$NODE_PKG" 2>/dev/null)" "$NODE_PKG"

# --- crates/axilog-node/npm/<platform>/package.json ---
NPM_DIR="$REPO_ROOT/crates/axilog-node/npm"
if [ -d "$NPM_DIR" ]; then
  shopt -s nullglob
  for platform_pkg in "$NPM_DIR"/*/package.json; do
    platform_name="$(basename "$(dirname "$platform_pkg")")"
    check "crates/axilog-node/npm/$platform_name/package.json" \
      "$(json_version "$platform_pkg" 2>/dev/null)" "$platform_pkg"

    # Cross-check: the main package's optionalDependencies pin for this
    # platform package must also match (see json_optional_dep_version doc
    # comment above for why this belongs in a *version* check).
    dep_name="$(node -e '
      const fs = require("fs");
      process.stdout.write(JSON.parse(fs.readFileSync(process.argv[1], "utf8")).name);
    ' "$platform_pkg")"
    pinned="$(json_optional_dep_version "$NODE_PKG" "$dep_name" 2>/dev/null)"
    check "crates/axilog-node/package.json optionalDependencies[\"$dep_name\"]" \
      "$pinned" "$NODE_PKG"
  done
  shopt -u nullglob
else
  echo "MISMATCH: $NPM_DIR does not exist (expected napi-rs platform package dirs)" >&2
  FAIL=1
fi

# --- crates/axilog-py/pyproject.toml ---
PYPROJECT="$REPO_ROOT/crates/axilog-py/pyproject.toml"
check "crates/axilog-py/pyproject.toml" "$(toml_project_version "$PYPROJECT")" "$PYPROJECT"

echo
if [ "$FAIL" -ne 0 ]; then
  echo "FAILED: one or more versions are out of sync with workspace Cargo.toml ($CARGO_VERSION)." >&2
  exit 1
fi

echo "OK: all versions in sync ($CARGO_VERSION)."
