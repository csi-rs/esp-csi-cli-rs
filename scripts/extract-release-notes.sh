#!/usr/bin/env bash
# Extract the GitHub release body for a tag from RELEASE_NOTES.md.
#
# Usage:
#   extract-release-notes.sh <tag> [source_file] [output_file]
#
# RELEASE_NOTES.md must contain a section headed exactly:
#   ## v0.7.2
# (matching the git tag). Content runs until the next "## v" heading or EOF.

set -euo pipefail

TAG="${1:?tag required (e.g. v0.7.2)}"
SOURCE="${2:-RELEASE_NOTES.md}"
OUT="${3:-release-notes.md}"

if [[ ! -f "$SOURCE" ]]; then
  echo "missing $SOURCE" >&2
  exit 1
fi

HEADING="## ${TAG}"
awk -v heading="$HEADING" '
  $0 == heading { found = 1; next }
  found && /^## v[0-9]/ { exit }
  found { print }
' "$SOURCE" > "$OUT"

if [[ ! -s "$OUT" ]]; then
  echo "No release notes section '$HEADING' in $SOURCE" >&2
  echo "Add a section before tagging:" >&2
  echo "" >&2
  echo "$HEADING" >&2
  echo "" >&2
  echo "Highlights: ..." >&2
  echo "" >&2
  echo "Existing sections:" >&2
  grep '^## v' "$SOURCE" >&2 || true
  exit 1
fi

{
  echo "# esp-csi-cli-rs ${TAG}"
  echo ""
  cat "$OUT"
} > "${OUT}.tmp"
mv "${OUT}.tmp" "$OUT"

echo "Wrote ${OUT} ($(wc -l < "$OUT") lines)"
