# Release manifest (`manifest.json`)

GitHub releases for `esp-csi-cli-rs` ship prebuilt, per-chip flash images and a
`manifest.json` consumed by host tooling (for example proprietary flash SDKs) to
download and verify the correct binary for a detected chip.

## CI

The [Release firmware binaries](../.github/workflows/release.yml) workflow:

1. Triggers on tags matching `v*` (or manual `workflow_dispatch`).
2. Builds release ELFs for **esp32**, **esp32c3**, **esp32c5**, **esp32c6**, **esp32s3**
   (default `println` / `auto` features — same as the `cargo esp*-build` aliases).
3. Converts each ELF to a merged flash image with `espflash save-image --merge`.
4. Runs [`scripts/generate-release-manifest.sh`](../scripts/generate-release-manifest.sh)
   to write `manifest.json` (SHA-256 per file).
5. Publishes all `.bin` files and `manifest.json` to the GitHub release.

### Cut a release

1. Bump `version` in `Cargo.toml`.
2. Add a **`## vX.Y.Z`** section at the top of [`RELEASE_NOTES.md`](../RELEASE_NOTES.md).
3. Tag and push:

```bash
git tag v0.7.0
git push origin v0.7.0
```

The workflow extracts that section into the GitHub release description
(`scripts/extract-release-notes.sh`). If the section is missing, publish fails.

Or run the workflow manually from the Actions tab and supply the tag.

## Manifest schema

```json
{
  "version": "0.7.0",
  "assets": {
    "esp32": {
      "url": "https://github.com/csi-rs/esp-csi-cli-rs/releases/download/v0.7.0/esp-csi-cli-rs-esp32.bin",
      "sha256": "<hex>",
      "flash_address": 0
    },
    "esp32c3": { "url": "...", "sha256": "...", "flash_address": 0 },
    "esp32c5": { "url": "...", "sha256": "...", "flash_address": 0 },
    "esp32c6": { "url": "...", "sha256": "...", "flash_address": 0 },
    "esp32s3": { "url": "...", "sha256": "...", "flash_address": 0 }
  }
}
```

| Field | Description |
|-------|-------------|
| `version` | Crate semver (tag without leading `v`) |
| `assets` | Map keyed by firmware `chip=` string |
| `url` | Direct download URL on the GitHub release |
| `sha256` | Lowercase hex digest of the `.bin` file |
| `flash_address` | Byte offset for `espflash write-bin` (merged images use `0`) |

### Chip keys

| Key | Target |
|-----|--------|
| `esp32` | ESP32 (`xtensa-esp32-none-elf`) |
| `esp32c3` | ESP32-C3 (`riscv32imc-unknown-none-elf`) |
| `esp32c5` | ESP32-C5 (`riscv32imac-unknown-none-elf`) |
| `esp32c6` | ESP32-C6 (`riscv32imac-unknown-none-elf`) |
| `esp32s3` | ESP32-S3 (`xtensa-esp32s3-none-elf`) |

## Local manifest generation

After building binaries into `dist/`:

```bash
chmod +x scripts/generate-release-manifest.sh
./scripts/generate-release-manifest.sh 0.7.0 v0.7.0 dist
```

Expected inputs:

```
dist/esp-csi-cli-rs-esp32.bin
dist/esp-csi-cli-rs-esp32c3.bin
dist/esp-csi-cli-rs-esp32c5.bin
dist/esp-csi-cli-rs-esp32c6.bin
dist/esp-csi-cli-rs-esp32s3.bin
```
