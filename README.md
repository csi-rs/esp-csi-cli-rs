# esp-csi-cli-rs

A command-line interface for collecting Wi-Fi **Channel State Information (CSI)** on ESP devices.
Flash it and configure the board over serial — no application code to write.

Built on [`esp-csi-rs`](https://github.com/csi-rs/esp-csi-rs), and exposes everything that crate
does. Supported devices: ESP32, ESP32-C3, ESP32-C5, ESP32-C6, ESP32-S3.

<div align="center">

![CLI Snapshot](/assets/cli_snapshot.png)

</div>

## The node model

`set-wifi --mode=` selects the node's **operational mode** — how it reaches the channel. It is one
of four independent attributes describing a node; the other three (network role, collection mode,
session role) come with the mode, because which values a mode admits depends on the mode.

| Mode | Reaches the channel by |
|---|---|
| `station` | associating to an AP or a commercial router |
| `sniffer` | promiscuous capture on a locked channel |
| `wifi-ap` | a self-contained softAP with DHCP |
| `ht20-emitter` / `ht40-emitter` | unassociated raw 802.11n injection, 20 or 40 MHz |
| `esp-now-central` / `esp-now-peripheral` | the symmetric connectionless exchange; both ends capture |
| `esp-now-fast-source` / `esp-now-fast-collector` | the asymmetric exchange, at the highest achievable CSI rate. Also spelled `esp-now-simplex-source` / `esp-now-simplex-peer` |

The model is documented once, in
[`esp-csi-rs/docs/network-model.md`](https://github.com/csi-rs/esp-csi-rs/blob/main/docs/network-model.md).
This README does not restate it.

## Features

- **Traffic generation** at a configurable rate.
- **Fine-grained CSI control** — LLTF, HTLTF, STBC HTLTF, LTF merge, and the per-chip acquisition
  knobs.
- **Runtime delivery switching** between async-queued, inline callback and off, without reflashing.
- **CSI output gate** — `set-csi-output --enabled=false` keeps capture and its timing running while
  suppressing all decoding and logging.
- **IO task control** — switch the TX or RX direction off to prune whole task subtrees.
- **Statistics** — `show-stats` reports PPS, rates and drops on demand.
- **Output formats** — human-readable text, compact array-list, binary serialized, or
  ESP32-CSI-Tool-compatible CSV.
- **Timed or indefinite collection**, with `q` to stop a run early without resetting the board.

## Requirements

- An ESP development board from the list above.
- [Rust with ESP target support](https://docs.esp-rs.org/book/installation/index.html).
- [`espflash`](https://docs.esp-rs.org/book/tooling/espflash.html) for flashing and monitoring. It
  decodes `defmt` frames out of the box.

## Prebuilt binaries

Tagged releases (`v*`) publish per-chip `.bin` flash images and a `manifest.json` for automated
host-side flashing — see [`docs/RELEASE_MANIFEST.md`](docs/RELEASE_MANIFEST.md).

## Build and flash

```bash
git clone https://github.com/csi-rs/esp-csi-cli-rs
cd esp-csi-cli-rs
cargo esp32c6          # build + flash + monitor
```

| Device | `println` (default) | `defmt` |
|---|---|---|
| ESP32 | `cargo esp32` | `cargo esp32-defmt` |
| ESP32-C3 | `cargo esp32c3` | `cargo esp32c3-defmt` |
| ESP32-C5 | `cargo esp32c5` | `cargo esp32c5-defmt` |
| ESP32-C6 | `cargo esp32c6` | `cargo esp32c6-defmt` |
| ESP32-S3 | `cargo esp32s3` | `cargo esp32s3-defmt` |

Append `-build` to any alias to compile without flashing. The `*-defmt` aliases swap the features,
the linker script and the espflash runner automatically — no `.cargo/config.toml` edits.

Flashing is needed once. Afterwards `espflash monitor` reconnects; `ctrl+R` resets the board and
`ctrl+C` ends the session.

### Cargo features

| Feature | Description |
|---|---|
| `esp32`, `esp32c3`, `esp32c5`, `esp32c6`, `esp32s3` | Target chip — pick exactly one |
| `println` | Log via `println!` (default) |
| `defmt` | Log via `defmt` (compact binary framing) |
| `auto` | Select the JTAG or UART backend at runtime (default) |
| `async-print` | Non-blocking async logging (implied by `jtag-serial`) |
| `statistics` | Expose PPS / rate / drop counters via `show-stats` (default) |
| `jtag-serial` | Force the JTAG backend |
| `uart` | Force the UART backend — do not combine with `async-print` |

## Things that catch people out

- SSIDs and passwords containing spaces must be quoted: `--sta-ssid='My WiFi'` or
  `--sta-ssid="My WiFi"`. Underscores pass through literally.
- **On the ESP32-C5, a 5 GHz sniffer can return a frozen IQ buffer** (a driver bug). Use the
  `wifi-ap` + `station` pair instead.
- For high-rate capture: `set-log-mode --mode=serialized`, `set-io-tasks --tx=off` on the station,
  and `set-traffic --frequency-hz=4000` on the AP. Avoid `set-csi-delivery --mode=callback` during
  capture — it caps output near 10 Hz. See [`docs/throughput.md`](docs/throughput.md).
- `q` stops a running collection, including an indefinite one. It is polled by the main loop every
  5 ms, not from the Wi-Fi callback.

## Documentation

| Document | What is in it |
|---|---|
| [`docs/commands.md`](docs/commands.md) | Every console command and its flags |
| [`docs/configuration-examples.md`](docs/configuration-examples.md) | Worked setups, as command sequences |
| [`docs/throughput.md`](docs/throughput.md) | What limits the observed CSI rate, and why it is usually the console |
| [`docs/collection-format.md`](docs/collection-format.md) | Reading the first rows of a collection |
| [`docs/defmt.md`](docs/defmt.md) | Logging with `defmt` |
| [`docs/RELEASE_MANIFEST.md`](docs/RELEASE_MANIFEST.md) | Release image manifest schema |
| [esp-csi-rs on docs.rs](https://docs.rs/esp_csi_rs) | The underlying library API |

## Development

Early development, `no-std` only. Contributions and suggestions are welcome.

## License

Copyright 2026 The csi-rs Team. Licensed under the Apache License, Version 2.0 — see
[`LICENSE`](LICENSE).

---

Made with 🦀 for ESP chips
