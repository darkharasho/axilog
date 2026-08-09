#!/usr/bin/env bash
# Local validation for scripts/package-release.sh (M8 Task 1). Exercises the
# packaging helper against the workspace's own debug binary (no release
# build required) and asserts: correct archive naming, the binary is present
# and executable inside the archive, and the generated checksum file is
# valid per `sha256sum -c`. Also covers the windows/.zip branch and a
# missing-binary failure case.
#
# Release-path-only tooling: intentionally NOT wired into ci.yml. Run
# manually (or ahead of cutting a release) via:
#   scripts/test-package-release.sh
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_SCRIPT="${REPO_ROOT}/scripts/package-release.sh"

PASS=0
FAIL=0

ok() {
  PASS=$((PASS + 1))
  echo "  ok - $1"
}

bad() {
  FAIL=$((FAIL + 1))
  echo "  FAIL - $1"
}

WORKDIR="$(mktemp -d)"
cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

echo "== building debug binary (cargo build -p axilog-cli) =="
if ! (cd "$REPO_ROOT" && cargo build -p axilog-cli) >"$WORKDIR/cargo-build.log" 2>&1; then
  echo "cargo build failed:"
  cat "$WORKDIR/cargo-build.log"
  exit 1
fi

DEBUG_BIN="${REPO_ROOT}/target/debug/axilog"
if [ ! -f "$DEBUG_BIN" ]; then
  echo "error: expected debug binary at $DEBUG_BIN not found after build" >&2
  exit 1
fi

VERSION="$("${REPO_ROOT}/scripts/workspace-version.sh")"
HOST_TARGET="$(rustc -vV | sed -n 's/^host: //p')"

echo "== version=$VERSION host_target=$HOST_TARGET =="

# --- Case 1: native (tar.gz) target -----------------------------------
echo "== case 1: native target ($HOST_TARGET) -> tar.gz =="
OUT1="$WORKDIR/out-native"
if OUTPUT="$("$PACKAGE_SCRIPT" "$DEBUG_BIN" "$VERSION" "$HOST_TARGET" "$OUT1" 2>&1)"; then
  ok "package-release.sh exits 0 for native target"
else
  bad "package-release.sh exited non-zero for native target: $OUTPUT"
fi

EXPECTED_ARCHIVE="axilog-${VERSION}-${HOST_TARGET}.tar.gz"
ARCHIVE_PATH="$OUT1/$EXPECTED_ARCHIVE"
if [ -f "$ARCHIVE_PATH" ]; then
  ok "archive named $EXPECTED_ARCHIVE exists"
else
  bad "expected archive $ARCHIVE_PATH not found (dir contents: $(ls "$OUT1" 2>/dev/null))"
fi

EXTRACT1="$WORKDIR/extract-native"
mkdir -p "$EXTRACT1"
if tar -xzf "$ARCHIVE_PATH" -C "$EXTRACT1" 2>/dev/null; then
  ok "archive extracts cleanly"
else
  bad "archive failed to extract"
fi

if [ -f "$EXTRACT1/axilog" ]; then
  ok "extracted archive contains 'axilog'"
  if [ -x "$EXTRACT1/axilog" ]; then
    ok "'axilog' is executable"
  else
    bad "'axilog' is present but not executable"
  fi
else
  bad "extracted archive missing 'axilog' binary"
fi

CHECKSUM_PATH="${ARCHIVE_PATH}.sha256"
if [ -f "$CHECKSUM_PATH" ]; then
  ok "checksum file $EXPECTED_ARCHIVE.sha256 exists"
  if grep -qE "^[0-9a-f]{64}  ${EXPECTED_ARCHIVE}\$" "$CHECKSUM_PATH"; then
    ok "checksum file has sha256sum-compatible format"
  else
    bad "checksum file content not in expected '<hash>  <filename>' format: $(cat "$CHECKSUM_PATH")"
  fi
  if (cd "$OUT1" && sha256sum -c "$(basename "$CHECKSUM_PATH")") >"$WORKDIR/sha256-check.log" 2>&1; then
    ok "sha256sum -c verifies the archive"
  else
    bad "sha256sum -c failed: $(cat "$WORKDIR/sha256-check.log")"
  fi
else
  bad "expected checksum file $CHECKSUM_PATH not found"
fi

# --- Case 2: windows target -> zip -------------------------------------
echo "== case 2: windows target (x86_64-pc-windows-msvc) -> zip =="
WIN_TARGET="x86_64-pc-windows-msvc"
OUT2="$WORKDIR/out-windows"
if OUTPUT="$("$PACKAGE_SCRIPT" "$DEBUG_BIN" "$VERSION" "$WIN_TARGET" "$OUT2" 2>&1)"; then
  ok "package-release.sh exits 0 for windows target"
else
  bad "package-release.sh exited non-zero for windows target: $OUTPUT"
fi

EXPECTED_ZIP="axilog-${VERSION}-${WIN_TARGET}.zip"
ZIP_PATH="$OUT2/$EXPECTED_ZIP"
if [ -f "$ZIP_PATH" ]; then
  ok "archive named $EXPECTED_ZIP exists"
else
  bad "expected archive $ZIP_PATH not found (dir contents: $(ls "$OUT2" 2>/dev/null))"
fi

if unzip -l "$ZIP_PATH" 2>/dev/null | grep -q 'axilog\.exe$'; then
  ok "zip archive contains 'axilog.exe'"
else
  bad "zip archive missing 'axilog.exe' entry"
fi

ZIP_CHECKSUM_PATH="${ZIP_PATH}.sha256"
if [ -f "$ZIP_CHECKSUM_PATH" ] && (cd "$OUT2" && sha256sum -c "$(basename "$ZIP_CHECKSUM_PATH")") >"$WORKDIR/sha256-check-zip.log" 2>&1; then
  ok "sha256sum -c verifies the zip archive"
else
  bad "zip checksum missing or invalid: $(cat "$WORKDIR/sha256-check-zip.log" 2>/dev/null)"
fi

# --- Case 3: missing binary fails loudly --------------------------------
echo "== case 3: missing binary path fails =="
OUT3="$WORKDIR/out-missing"
if "$PACKAGE_SCRIPT" "$WORKDIR/does-not-exist" "$VERSION" "$HOST_TARGET" "$OUT3" >"$WORKDIR/missing.log" 2>&1; then
  bad "package-release.sh should have failed for a missing binary but exited 0"
else
  ok "package-release.sh exits non-zero for a missing binary"
fi

echo
echo "== summary: $PASS passed, $FAIL failed =="
if [ "$FAIL" -ne 0 ]; then
  exit 1
fi
