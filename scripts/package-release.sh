#!/usr/bin/env bash
# Package a built axilog CLI binary into a release archive + checksum file
# (M8 Task 1). Unit-tested by scripts/test-package-release.sh; invoked once
# per matrix leg from .github/workflows/release.yml.
#
# Usage: package-release.sh <binary-path> <version> <target> <outdir>
#   binary-path  path to the already-built axilog (or axilog.exe) binary
#   version      release version, e.g. 0.1.0 (no leading "v")
#   target       Rust target triple, e.g. x86_64-unknown-linux-gnu
#   outdir       directory to write the archive + checksum into (created if
#                missing)
#
# Output (in outdir):
#   axilog-<version>-<target>.tar.gz   (or .zip if <target> contains "windows")
#   axilog-<version>-<target>.tar.gz.sha256  (sha256sum-compatible, relative
#                                              filename only, verifiable via
#                                              `sha256sum -c` from inside outdir)
#
# Archive layout: the binary is packaged alone at the archive root, as
# `axilog` (or `axilog.exe` for windows targets) regardless of the input
# file's own name -- callers may pass a debug binary built under a different
# name/path (see the local test script) as long as the target triple is
# supplied explicitly.
set -euo pipefail

usage() {
  echo "Usage: $0 <binary-path> <version> <target> <outdir>" >&2
  exit 1
}

[ $# -eq 4 ] || usage

BINARY_PATH="$1"
VERSION="$2"
TARGET="$3"
OUTDIR="$4"

if [ ! -f "$BINARY_PATH" ]; then
  echo "error: binary not found at '$BINARY_PATH'" >&2
  exit 1
fi

if [ -z "$VERSION" ]; then
  echo "error: version must not be empty" >&2
  exit 1
fi

if [ -z "$TARGET" ]; then
  echo "error: target must not be empty" >&2
  exit 1
fi

mkdir -p "$OUTDIR"
OUTDIR="$(cd "$OUTDIR" && pwd)"

# Windows targets get a .zip (unzip is universally available there); every
# other target gets a .tar.gz that preserves the executable bit.
case "$TARGET" in
  *windows*)
    IS_WINDOWS=1
    BIN_NAME="axilog.exe"
    ;;
  *)
    IS_WINDOWS=0
    BIN_NAME="axilog"
    ;;
esac

STAGE_DIR="$(mktemp -d)"
trap 'rm -rf "$STAGE_DIR"' EXIT

cp "$BINARY_PATH" "$STAGE_DIR/$BIN_NAME"
chmod +x "$STAGE_DIR/$BIN_NAME"

ARCHIVE_STEM="axilog-${VERSION}-${TARGET}"

if [ "$IS_WINDOWS" -eq 1 ]; then
  ARCHIVE_NAME="${ARCHIVE_STEM}.zip"
  if command -v zip >/dev/null 2>&1; then
    (cd "$STAGE_DIR" && zip -q -X "${OUTDIR}/${ARCHIVE_NAME}" "$BIN_NAME")
  elif command -v 7z >/dev/null 2>&1; then
    # windows-latest runners ship 7-Zip on PATH but not Info-Zip's `zip`;
    # `7z a -tzip` produces an equivalent standard zip archive.
    (cd "$STAGE_DIR" && 7z a -tzip -bd "${OUTDIR}/${ARCHIVE_NAME}" "$BIN_NAME" >/dev/null)
  else
    echo "error: neither 'zip' nor '7z' found on PATH; cannot create $ARCHIVE_NAME" >&2
    exit 1
  fi
else
  ARCHIVE_NAME="${ARCHIVE_STEM}.tar.gz"
  tar -czf "${OUTDIR}/${ARCHIVE_NAME}" -C "$STAGE_DIR" "$BIN_NAME"
fi

CHECKSUM_NAME="${ARCHIVE_NAME}.sha256"
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$OUTDIR" && sha256sum "$ARCHIVE_NAME" > "$CHECKSUM_NAME")
elif command -v shasum >/dev/null 2>&1; then
  # macOS runners ship BSD tools only -- no sha256sum -- but `shasum -a 256`
  # emits the identical "<hash>  <filename>" format, so the resulting file
  # stays verifiable with GNU `sha256sum -c` on the (ubuntu) release job.
  (cd "$OUTDIR" && shasum -a 256 "$ARCHIVE_NAME" > "$CHECKSUM_NAME")
else
  echo "error: neither 'sha256sum' nor 'shasum' found on PATH; cannot checksum $ARCHIVE_NAME" >&2
  exit 1
fi

echo "Packaged: ${OUTDIR}/${ARCHIVE_NAME}"
echo "Checksum: ${OUTDIR}/${CHECKSUM_NAME}"
