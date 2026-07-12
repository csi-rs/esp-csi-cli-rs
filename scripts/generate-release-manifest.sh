#!/usr/bin/env bash
# Build manifest.json for a GitHub release from prebuilt per-chip .bin files.
#
# Usage:
#   generate-release-manifest.sh <version> <tag> <dist_dir> [github_repo]
#
# Expects:
#   dist/esp-csi-cli-rs-{esp32,esp32c3,esp32c5,esp32c6,esp32s3}.bin
#
# Writes:
#   dist/manifest.json

set -euo pipefail

VERSION="${1:?version required (e.g. 0.7.2)}"
TAG="${2:?tag required (e.g. v0.7.2)}"
DIST="${3:?dist directory required}"
REPO="${4:-${GITHUB_REPOSITORY:-csi-rs/esp-csi-cli-rs}}"

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

CHIPS=(esp32 esp32c3 esp32c5 esp32c6 esp32s3)
ASSETS="{}"

for chip in "${CHIPS[@]}"; do
  bin="${DIST}/esp-csi-cli-rs-${chip}.bin"
  if [[ ! -f "$bin" ]]; then
    echo "missing artifact: $bin" >&2
    exit 1
  fi
  sha256=$(sha256sum "$bin" | awk '{print $1}')
  url="https://github.com/${REPO}/releases/download/${TAG}/esp-csi-cli-rs-${chip}.bin"
  ASSETS=$(jq -n \
    --argjson base "$ASSETS" \
    --arg chip "$chip" \
    --arg url "$url" \
    --arg sha "$sha256" \
    '$base + {($chip): {url: $url, sha256: $sha, flash_address: 0}}')
done

jq -n \
  --arg version "$VERSION" \
  --argjson assets "$ASSETS" \
  '{version: $version, assets: $assets}' \
  > "${DIST}/manifest.json"

echo "Wrote ${DIST}/manifest.json"
jq . "${DIST}/manifest.json"
