#!/usr/bin/env bash
#
# Build the GitHub Release body for a tag.
#
# Notes come from the hand-written `## <tag> — <date>` section in
# docs/CHANGELOG.md, the same shape AxiBridge uses with RELEASE_NOTES.md, and
# for the same reason: `gh release --generate-notes` emits a raw commit list,
# which reads as a diff rather than as an explanation of what changed for the
# people installing it.
#
# Missing section == hard failure. A release published with empty or stale
# notes is worse than a release that failed loudly and got re-run.
#
# The install/verify footer below is generated rather than written into the
# changelog, because it is identical every release except for the version and
# would rot the moment someone forgot to bump it by hand.
#
# Usage: scripts/release-notes.sh [--no-footer] v1.0.0 [> notes.md]
#
# --no-footer prints the changelog prose alone. The Discord embed uses it: the
# footer is boilerplate that would eat most of the 4096-character budget.
set -euo pipefail

footer=1
if [ "${1:-}" = "--no-footer" ]; then
    footer=0
    shift
fi

tag="${1:-}"
if [ -z "$tag" ]; then
    echo "usage: $0 [--no-footer] <tag>" >&2
    exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
changelog="$repo_root/docs/CHANGELOG.md"

if [ ! -f "$changelog" ]; then
    echo "::error::$changelog not found" >&2
    exit 1
fi

# Capture everything between this tag's `## <tag>` heading and the next `## `
# heading. Anchored on `## ` specifically so the `# Changelog` title and any
# `### ` sub-headings inside a section are not treated as boundaries.
body=$(awk -v tag="$tag" '
    /^## / {
        # $2 is the version token in "## v1.0.0 — 2026-08-18".
        capture = ($2 == tag) ? 1 : 0
        next
    }
    capture { print }
' "$changelog")

if [ -z "$(printf '%s' "$body" | tr -d '[:space:]')" ]; then
    echo "::error::No docs/CHANGELOG.md section found for $tag. Add a '## $tag — <date>' entry and re-run." >&2
    exit 1
fi

version="${tag#v}"

# Trim leading/trailing blank lines off the captured section so the footer
# below sits one blank line from the prose, not five.
printf '%s\n' "$body" | sed -e '/./,$!d' | tac | sed -e '/./,$!d' | tac

[ "$footer" -eq 1 ] || exit 0

cat <<EOF

---

## Install

\`\`\`bash
# Node
npm install @axiapps/axilog@$version

# Python
pip install axilog==$version

\`\`\`

The CLI ships as a static binary — download the archive for your platform from
the assets below, unpack, and run. No runtime to install alongside it.

## Verify

Every archive is listed in \`SHA256SUMS\`, attached to this release:

\`\`\`bash
sha256sum -c SHA256SUMS --ignore-missing
\`\`\`
EOF
