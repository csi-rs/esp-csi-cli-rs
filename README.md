# esp-csi-cli-rs

A command-line interface for collecting Wi-Fi **Channel State Information (CSI)** on ESP devices.
Flash it and configure the board over serial — no application code to write.

Built on [`esp-csi-rs`](https://github.com/csi-rs/esp-csi-rs), and exposes everything that crate
does. Supported devices: ESP32, ESP32-C3, ESP32-C5, ESP32-C6, ESP32-S3.

<div align="center">

![CLI Snapshot](/assets/cli_snapshot.png)

</div>

## The node model

The CLI uses IEEE 802.11bf terms. `set-wifi --mode=` selects the node's **mode** — how it reaches
the channel. The **session role** (initiator, responder, or none for an observer) comes with the
mode (the `--mode` token names the end), and so does the **sensing role** (transmitter, receiver,
or both). The **reporting** is set with `set-wifi --reporting=report|none|threshold|decimate`
(`--report-to=host|local`; parameters via `set-reporting`) on the modes that admit a choice
(`station`, `wifi-ap`, `esp-now-initiator`, `esp-now-responder`, and the sniffer for all but
`none`); the others fix it. The **measurement setup** (`set-measurement --periodicity-hz=`) sets the
rate of the sounding traffic. The host is the controller: it starts and stops the run, and can
name it with `set-session`. `show-config` prints all of these for the next run.

| Mode | Reaches the channel by |
|---|---|
| `station` | associating to an AP or a commercial router |
| `sniffer` | promiscuous capture on a locked channel |
| `wifi-ap` | a self-contained softAP with DHCP |
| `ht20-emitter` / `ht40-emitter` | unassociated raw 802.11n injection, 20 or 40 MHz (HE20 with `--he20=on` on the C5/C6) |
| `esp-now-initiator` / `esp-now-responder` | the symmetric connectionless exchange; both ends capture |
| `esp-now-simplex-source` / `esp-now-simplex-peer` | the asymmetric exchange, at the highest achievable CSI rate |

The model is documented once, in
[`esp-csi-rs/docs/network-model.md`](https://github.com/csi-rs/esp-csi-rs/blob/main/docs/network-model.md).
This README does not restate it.

## Features

- **Traffic generation** at a configurable periodicity.
- **Fine-grained CSI control** — LLTF, HTLTF, STBC HTLTF, LTF merge, and the per-chip acquisition
  knobs.
- **Runtime delivery switching** between async-queued, inline callback and off, without reflashing.
- **CSI output gate** — `set-csi-output --enabled=false` keeps capture and its timing running while
  suppressing all decoding and logging.
- **IO task control** — switch the TX or RX direction off to prune whole task subtrees.
- **Statistics** — `show-stats` reports PPS, rates and drops (broken down by cause) on demand.
- **Output formats** — human-readable text, compact array-list, binary serialized (the versioned
  `esp_csi_rs::wire` format), or ESP32-CSI-Tool-compatible CSV.
- **Reporting** — report every measurement (to the host or only on-device), none, only while the
  channel moves (802.11bf threshold-based reporting), or every n-th frame (decimate).
- **802.11ax HE20 on the ESP32-C5/C6** — `--he20=on` for emitters and the ESP-NOW pair,
  `set-protocol --protocol=ax` for station / AP / sniffer, `set-csi --preset=he20` for receivers.
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
  and `set-measurement --periodicity-hz=4000` on the AP. Avoid `set-csi-delivery --mode=callback` during
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
