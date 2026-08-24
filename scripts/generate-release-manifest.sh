#!/usr/bin/env bash
# Build manifest.json for a GitHub release from prebuilt per-chip .bin files.
#
# Usage:
#   generate-release-manifest.sh <version> <tag> <dist_dir> [github_repo] [baud]
#
# `baud` is the console rate the images were BUILT at (`ESP_CSI_CLI_UART_BAUD`, default 115200).
# It belongs in the manifest because the firmware's own `info` block can only CONFIRM the rate once
# a host is already talking at it — tooling that has to pick a rate before the first byte has
# nowhere else to learn it, and a host opening the port at the wrong rate reads garbage that looks
# like a hardware fault rather than a settings mismatch.
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
BAUD="${5:-${ESP_CSI_CLI_UART_BAUD:-115200}}"

case "$BAUD" in
  ''|*[!0-9]*) echo "baud must be an integer, got: $BAUD" >&2; exit 1 ;;
esac

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
    --argjson baud "$BAUD" \
    '$base + {($chip): {url: $url, sha256: $sha, flash_address: 0, baud: $baud}}')
done

jq -n \
  --arg version "$VERSION" \
  --argjson assets "$ASSETS" \
  '{version: $version, assets: $assets}' \
  > "${DIST}/manifest.json"

echo "Wrote ${DIST}/manifest.json"
jq . "${DIST}/manifest.json"
