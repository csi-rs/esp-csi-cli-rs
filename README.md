# esp-csi-cli-rs

`esp-csi-cli-rs` is a command-line interface (CLI) application that runs on top of the `esp-csi-rs` crate.  `esp-csi-cli-rs` provides a user friendly interface for configuring and collecting Wi-Fi Channel State Information (CSI) on ESP devices. It allows users to configure various parameters related to CSI data collection.

In order to use this crate, you would need to flash the source code for your target device. Currently supported devices include:

- ESP32
- ESP32-C3
- ESP32-C5
- ESP32-C6
- ESP32-S3

<div align="center">

![CLI Snapshot](/assets/cli_snapshot.png)

</div>

## Features

* **Emitter / collector roles:** every mode either EMITS known RF energy (never captures) or COLLECTS the channel response. Collector modes: Sniffer, Station, softAP (`wifi-ap`). Emitter modes: `ht20-emitter` / `ht40-emitter` (802.11n, all chips).
* **Traffic Generation:** Generate traffic at configurable intervals.
* **Fine-grained CSI Control:** Enable or disable specific CSI features like LLTF, HTLTF, STBC HTLTF, and LTF Merge.
* **IO Task Control:** Toggle TX or RX direction tasks at the CLI.
* **Runtime Delivery Switching:** Flip CSI delivery between async-queued, inline callback, and off without re-flashing.
* **Statistics Snapshot:** `show-stats` reports PPS, rates, and drops on demand.
* **CSI Output Gate:** `set-csi-output --enabled=false` keeps capture (and its timing) running while suppressing all decoding and logging.
* **Flexible Log Format:** Choose between human-readable text, compact array-list, binary serialized, or ESP-CSI-Tool-compatible CSV output.
* **CLI Control:** Interact with the device using simple commands over a serial connection.
* **Early Stop:** Press `q` to abort a running collection — even an indefinite one — without resetting the board.
* **Configuration Management:** Show the current configuration or reset to defaults.
* **Timed Collection:** Start CSI collection for a specific duration or run indefinitely.
* **Flexible Logging:** Supports standard `println!` or the more efficient `defmt` logging.


## Requirements

* **Hardware:** An ESP development board (ESP32, ESP32-C3, ESP32-C6, or ESP32-S3).
* **Rust** with ESP target support — full setup guide available [here](https://docs.esp-rs.org/book/installation/index.html).
* **`espflash`** for flashing and monitoring — installation instructions available [here](https://docs.esp-rs.org/book/tooling/espflash.html). `espflash` also supports `defmt` log decoding out of the box.

## Prebuilt release binaries

Tagged releases (`v*`) publish per-chip `.bin` flash images and a
[`manifest.json`](docs/RELEASE_MANIFEST.md) for automated host-side flashing.
See [docs/RELEASE_MANIFEST.md](docs/RELEASE_MANIFEST.md) for the schema and CI
details.

## Usage

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/csi-rs/esp-csi-cli-rs
    cd esp-csi-cli-rs
    ```

2.  **Build & Flash** using the provided Cargo aliases — one command builds, flashes, and opens the monitor:

    | Device   | `println` (default) | `defmt`                 |
    |----------|---------------------|-------------------------|
    | ESP32    | `cargo esp32`       | `cargo esp32-defmt`     |
    | ESP32-C3 | `cargo esp32c3`     | `cargo esp32c3-defmt`   |
    | ESP32-C5 | `cargo esp32c5`     | `cargo esp32c5-defmt`   |
    | ESP32-C6 | `cargo esp32c6`     | `cargo esp32c6-defmt`   |
    | ESP32-S3 | `cargo esp32s3`     | `cargo esp32s3-defmt`   |

    To build without flashing, append `-build` to any of the above (e.g. `cargo esp32c6-build`, `cargo esp32s3-defmt-build`, etc.).

    The `*-defmt` variants automatically: drop the `println` feature, enable `defmt`, append `-Tdefmt.x` to the linker script set, and pass `--log-format defmt` to `espflash` so the monitor decodes binary log frames. No `.cargo/config.toml` editing required.

    > 📝 The plain aliases default to `println` logging. For `defmt` builds use the parallel `*-defmt` (run + flash + monitor with frame decoding) and `*-defmt-build` (build only) aliases — they swap in the right features, runner, and linker script automatically. See [Enabling `defmt` Logging](#enabling-logging-w-defmt) for details.

    **Custom builds** — if you need finer control over features, you can invoke `cargo build` directly. The full set of available features is:

    | Feature       | Description                                                          |
    |---------------|----------------------------------------------------------------------|
    | `esp32`       | Target: ESP32                                                        |
    | `esp32c3`     | Target: ESP32-C3                                                     |
    | `esp32c5`     | Target: ESP32-C5                                                     |
    | `esp32c6`     | Target: ESP32-C6                                                     |
    | `esp32s3`     | Target: ESP32-S3                                                     |
    | `println`     | Log via `println!` (default)                                         |
    | `defmt`       | Log via `defmt` (efficient binary logging)                           |
    | `auto`        | Auto-select JTAG or UART backend at runtime (default)                |
    | `async-print` | Non-blocking async logging (auto-enabled by `jtag-serial`)           |
    | `statistics`  | Expose runtime PPS / rate / drop counters via `show-stats` (default) |
    | `jtag-serial` | Force JTAG serial backend (auto-enables `async-print`)               |
    | `uart`        | Force UART backend (do **not** combine with `async-print`)          |

    ```bash
    # Example: ESP32-C6, forced JTAG backend (auto-enables async-print)
    cargo build --no-default-features --features "no-std,esp32c6,println,jtag-serial,statistics" \
        --target riscv32imac-unknown-none-elf --release
    ```

3.  **Monitor** (if you used a `-build` alias or a manual `cargo build`): Connect your ESP device over USB and run:
    ```bash
    espflash flash --monitor
    ```
    For `defmt` builds, pass the ELF file to enable log decoding:
    ```bash
    espflash flash --monitor --log-format defmt
    ```

> 📝 `defmt` builds require a monitoring tool capable of interpreting `defmt` encoding, such as `espflash`. Without it you will observe garbled output. The monitor requires the original ELF to decode incoming log frames.

> 🛑 Flashing is only required once. After disconnecting and reconnecting the device, run `espflash monitor` then press `ctrl+R` to reset.

> 🛑 If you encounter strange behaviour with the CLI, press `ctrl+R` to reset the device. Press `ctrl+C` to terminate the session — you will need to run `espflash monitor` again to reconnect.

## CLI Commands

This is a list of commands available through the CLI interface:
> 📝 The `set-csi` command options differ on the ESP32-C5 and ESP32-C6 (which expose per-PPDU-format acquisition flags — legacy / HT20 / HT40, plus VHT20 and forced L-LTF on C5 — instead of the classic LLTF/HTLTF flags).

* **`help [command]`**
    * Description: Display the main help menu or details for a specific command.
    * Example: `help set-wifi`

* **`set-traffic [OPTIONS]`**
    * Description: Configure traffic generation parameters.
    * Options:
        * `--frequency-hz=<NUMBER>`: Specify the traffic frequency in Hertz (default: 100). Set to `0` to disable traffic generation.
    * Examples:
        * `set-traffic --frequency-hz=10`
        * `set-traffic --frequency-hz=0`

* **`set-csi-output [OPTIONS]`**
    * Description: Toggle off-device delivery of captured CSI. With delivery off the radio still captures — RX path and timing unchanged — but nothing is decoded, logged, or handed to a callback. No effect on an emitter, which captures nothing.
    * Options:
        * `--enabled=<true|false>`: Deliver captured CSI (default: `true`).
    * Examples:
        * `set-csi-output --enabled=true`
        * `set-csi-output --enabled=false`

* **`set-log-mode [OPTIONS]`**
    * Description: Set the CSI output logging format at runtime.
    * Options:
        * `--mode=<text|array-list|serialized|esp-csi-tool>`: Output format for CSI packets (default: `text`).
            * `text`: Verbose human-readable output with full metadata.
            * `array-list`: Compact CSV-style array, one line per packet — best for host-side data processing.
            * `serialized`: Binary COBS-framed postcard format — most compact, requires a compatible deserializer on the host.
            * `esp-csi-tool`: Hernandez-style 26-column CSV (`CSI_DATA,...` lines) compatible with the ESP32-CSI-Tool collector.
    * Examples:
        * `set-log-mode --mode=text`
        * `set-log-mode --mode=array-list`
        * `set-log-mode --mode=esp-csi-tool`

* **`set-csi [OPTIONS]`**
    * Description: Configure CSI feature flags. Each flag is an `on|off` toggle, so a feature can be re-enabled after being turned off (no `reset-config` needed). Accepted values: `on|off`, `true|false`, `1|0`, `enable|disable`, `yes|no`.
    * Options (ESP32, ESP32-C3, ESP32-S3):
        * `--lltf=<on|off>`: LLTF CSI (default: on).
        * `--htltf=<on|off>`: HTLTF CSI (default: on).
        * `--stbc-htltf=<on|off>`: STBC HTLTF CSI (default: on).
        * `--ltf-merge=<on|off>`: LTF Merge CSI (default: on).
    * Options (ESP32-C5, ESP32-C6):
        * `--csi=<on|off>`: Acquisition of CSI, master switch (default: on).
        * `--csi-legacy=<on|off>`: L-LTF acquisition for 11g PPDUs (default: on).
        * `--csi-ht20=<on|off>`: HT-LTF for HT20 PPDUs (default: on).
        * `--csi-ht40=<on|off>`: HT-LTF for HT40 PPDUs (default: on).
        * `--val-scale-cfg=<0-3>`: Value scale configuration (default: 2).
        * `--preset=<default>`: Apply a CSI acquisition preset.
        * `--dump-ack=<on|off>`: Dump 802.11 ACK frames (default: on).
        * `--csi-force-lltf=<on|off>`: Force L-LTF acquisition (ESP32-C5 only).
        * `--csi-vht=<on|off>`: VHT-LTF for VHT20 PPDUs (ESP32-C5 only).
    * Examples:
        * `set-csi --lltf=off --ltf-merge=off`
        * `set-csi --csi-legacy=off --preset=default`
        * `set-csi --csi-ht40=on --csi-ht20=off`

* **`set-wifi [OPTIONS]`**
    * Description: Configure WiFi and network settings. **Note:** SSIDs/passwords with spaces should be wrapped in single or double quotes (e.g. `--sta-ssid='My Network'` or `--sta-ssid="My Network"`). Both quote styles are interchangeable. Underscores (`_`) are passed through literally.
    * Options:
        * `--mode=<station|sniffer|wifi-ap|ht20-emitter|ht40-emitter>`: Specify WiFi operation mode (default: `sniffer`).
        * `--sta-ssid=<SSID>`: Set the SSID for Station mode.
        * `--sta-password=<PASSWORD>`: Set the password for Station mode.
        * `--ap-ssid=<SSID>`: Set the SSID for wifi-ap mode (default: `esp-csi-ap`).
        * `--ap-password=<PASSWORD>`: Set the softAP password (empty = open network).
        * `--ap-dhcp=<on|off>`: Enable/disable the built-in DHCP server in wifi-ap mode (default: on).
        * `--ap-leases=<1-8>`: DHCP lease pool size in wifi-ap mode (default: 4). With more than
          one lease the AP's ICMP flood round-robins across **all** associated stations, so every
          station captures CSI; `1` restores the legacy single-target flood.
        * `--ap-burst=<on|off>`: Synchronized burst flood in wifi-ap mode (default: off). Each
          flood tick sends one unicast frame back-to-back to **every** associated station, so all
          stations capture their downlink CSI within tens of microseconds of each other
          (time-aligned multi-receiver capture). Every station sees the full `frequency-hz`, so
          total offered airtime is `frequency-hz × leases` — lower the rate if the channel
          saturates. `off` keeps the round-robin flood (rate shared across stations).
        * `--set-channel=<NUMBER>`: Set the WiFi channel. Use 1–14 on 2.4 GHz; on ESP32-C5
          use 5 GHz channels such as 149 (default: 1 on most chips,
          149 on ESP32-C5).
        * `--peer-mac=<aa:bb:cc:dd:ee:ff>`: Emitter modes — destination address of injected frames. Unicasting to a collector usually raises that collector's CSI rate. Empty value = broadcast (default).
        * `--ht40=<above|below|none>`: `wifi-ap` mode — softAP secondary channel (default: `none` = HT20). This does **not** pick emitter bandwidth; use `--mode=ht40-emitter` for that.
        * `--inject-period-ms=<MS>`: Emitter modes — delay between injected frames (default: `20` ≈ 50 frames/s).
    * Examples:
        * `set-wifi --mode=sniffer --set-channel=6`
        * `set-wifi --mode=station --sta-ssid="My Network" --sta-password="my password"`
        * `set-wifi --mode=wifi-ap --set-channel=6 --ap-ssid=esp-csi-ap`
        * `set-wifi --mode=ht20-emitter --set-channel=6 --inject-period-ms=20`
        * `set-wifi --mode=ht40-emitter --set-channel=6 --peer-mac=aa:bb:cc:dd:ee:ff`

* **`start [OPTIONS]`**
    * Description: Start the CSI collection process. Ensure the device is configured first. Press `q` (or `Q`) on the serial console at any time to stop collection early.
    * Options:
        * `--duration=<SECONDS>`: Specify the duration in seconds. If omitted, collection runs indefinitely.
    * Examples:
        * `start`
        * `start --duration=120`

* **`show-config`**
    * Description: Display the current configuration settings for all parameters.
    * Example: `show-config`

* **`reset-config`**
    * Description: Reset all configurations to their default values.
    * Example: `reset-config`

* **`set-rate [OPTIONS]`** *(reporting only)*
    * Description: Record the Wi-Fi PHY rate for `show-config`. Nothing applies it: collector modes derive their rate from the surrounding radio configuration, and an emitter transmits at the rate its forced TX PHY implies.
    * Options:
        * `--rate=<NAME>`: One of `mcs0-lgi` (default), `mcs1-lgi`..`mcs7-lgi`, `mcs0-sgi`, `1m`, `2m`, `5m5`, `11m`, `6m`, `9m`, `12m`, `18m`, `24m`, `36m`, `48m`, `54m`.
    * Examples:
        * `set-rate --rate=mcs0-lgi`
        * `set-rate --rate=24m`

* **`set-io-tasks [OPTIONS]`**
    * Description: Toggle the TX and/or RX direction tasks. Useful for asymmetric topologies — disabling RX makes the node a pure transmitter (skips the WiFi-callback CSI path); disabling TX makes it a pure receiver (no traffic generation).
    * Options:
        * `--tx=<on|off>`: Enable or disable the TX task. Omit to keep the current state.
        * `--rx=<on|off>`: Enable or disable the RX task. Omit to keep the current state.
    * Examples:
        * `set-io-tasks --tx=off`         (receive-only)
        * `set-io-tasks --tx=on --rx=on`  (default)
    * Note: to stop CSI *delivery* while leaving the RX path and its timing intact, use `set-csi-output --enabled=false` instead.

* **`set-csi-delivery [OPTIONS]`**
    * Description: Switch the CSI delivery mode at runtime, and independently toggle the inline UART/JTAG log gate. The two delivery paths are mutually exclusive — the WiFi callback only ever pays for one per packet.
    * Options:
        * `--mode=<off|callback|async>`: `off` drops user delivery, `callback` invokes the registered `set_csi_callback` hook inline in the WiFi callback, `async` queues to `CSINodeClient::next_csi_packet` (default for the indefinite collection path).
        * `--logging=<on|off>`: Toggle the per-packet `log_csi` UART/JTAG gate independently.
    * Examples:
        * `set-csi-delivery --mode=async`
        * `set-csi-delivery --mode=off --logging=off`

* **`info`**
    * Description: Print a machine-parseable firmware identification block. Intended for host-side tooling that needs to verify which firmware is running on the device. The first line — `ESP-CSI-CLI/<version>` — is also emitted at the top of the welcome banner on every reset, so a host can identify the firmware passively without sending this command.
    * Output format:
        ```
        ESP-CSI-CLI/<version>
        name=esp-csi-cli-rs
        version=<version>
        chip=<esp32|esp32c3|esp32c5|esp32c6|esp32s3|unknown>
        protocol=<u32>
        features=<comma-separated-list>
        END-INFO
        ```
    * Example: `info`

* **`show-stats`** *(requires `statistics` feature, on by default)*
    * Description: Print a one-shot snapshot of runtime CSI / traffic counters: RX/TX packet totals, average PPS, RX/TX rate in Hz, and RX dropped packets. Counters reset on the start of each new `start` collection.
    * Example: `show-stats`

## CLI Configuration Examples

1.  **Configure an ESP as a WiFi Sniffer on channel 6 and collect indefinitely in array-list format:**
    ```
    set-wifi --mode=sniffer --set-channel=6
    set-log-mode --mode=array-list
    show-config
    start
    ```

2.  **Configure an ESP as a Station connected to an existing network and collect for 5 minutes:**
    ```
    set-wifi --mode=station --sta-ssid="My Router" --sta-password="router password"
    set-traffic --frequency-hz=20
    show-config
    start --duration=300
    ```

3.  **HT emitter + sniffer collector (any chip — the controlled pairing):**
    ```
    # Collector board (sniffer on the emitter's channel, no self-generated traffic)
    set-wifi --mode=sniffer --set-channel=6
    set-traffic --frequency-hz=0
    set-log-mode --mode=array-list
    start

    # Emitter board (use ht40-emitter for 40 MHz bonded)
    set-wifi --mode=ht20-emitter --set-channel=6 --inject-period-ms=20
    start
    ```

4.  **Keep a node's traffic on air without paying for CSI delivery, then check stats:**
    ```
    set-wifi --mode=wifi-ap --set-channel=6
    set-csi-output --enabled=false
    set-traffic --frequency-hz=4000
    start
    # ... after pressing 'q' to stop:
    show-stats
    ```

5.  **Emit ESP32-CSI-Tool-compatible CSV for a host pipeline:**
    ```
    set-wifi --mode=sniffer --set-channel=6
    set-log-mode --mode=esp-csi-tool
    start --duration=60
    ```

6.  **SoftAP lab pair (board A = AP collector, board B = station on same SSID):**
    ```
    # Board A (AP collector)
    set-wifi --mode=wifi-ap --set-channel=6 --ap-ssid=esp-csi-ap
    set-protocol --protocol=n
    set-traffic --frequency-hz=4000
    start

    # Board B (station — match AP SSID/channel)
    set-wifi --mode=station --sta-ssid=esp-csi-ap --set-channel=6
    set-protocol --protocol=n
    set-traffic --frequency-hz=4000
    start
    ```

7.  **5 GHz associated AP/STA pair (ESP32-C5 — serialized high-rate CSI):**
    ```
    # Board A (AP collector — 5 GHz ch149 on C5)
    set-wifi --mode=wifi-ap --set-channel=149 --ap-ssid=esp-csi-ap
    set-protocol --protocol=n
    set-traffic --frequency-hz=4000
    start

    # Board B (station RX — record serialized CSI)
    set-wifi --mode=station --sta-ssid=esp-csi-ap --set-channel=149
    set-protocol --protocol=n
    set-log-mode --mode=serialized
    set-io-tasks --tx=off
    start
    ```
    On ESP32-C5 the station channel doubles as the dual-band hint (2.4 GHz:
    `--set-channel=6`, 5 GHz: `--set-channel=149`). Disable station TX
    (`set-io-tasks --tx=off`) so the AP's downlink flood is not competing with a
    second ICMP generator.

    The pair scales to multiple stations: the AP's DHCP pool holds 4 leases by
    default (`set-wifi --ap-leases=<1-8>` to change) and the ICMP flood
    round-robins across all associated stations, so each one captures CSI. Note
    the offered rate is shared — with N stations each sees roughly
    `frequency-hz / N` packets per second.

    For **temporally-synchronized** multi-receiver captures, add
    `set-wifi --ap-burst=on` on the AP: every flood tick then fires one unicast
    frame back-to-back to each associated station, so all stations sample the
    channel within tens of microseconds of one another and each sees the full
    `frequency-hz` rate. (A single broadcast frame cannot be used instead — on
    an ESP32 softAP broadcast is DTIM-buffered and dropped under load.) Total
    offered airtime becomes `frequency-hz × N`, so lower
    `set-traffic --frequency-hz` if the channel saturates.

8.  **HT40 emitter unicasting to one collector (40 MHz sounding at ~100 frames/s):**
    ```
    # Collector board — note its MAC from the welcome banner / `info`
    set-wifi --mode=sniffer --set-channel=6
    set-traffic --frequency-hz=0
    set-log-mode --mode=serialized
    start

    # Emitter board — unicast to that collector, 10 ms period
    set-wifi --mode=ht40-emitter --set-channel=6 --peer-mac=aa:bb:cc:dd:ee:ff --inject-period-ms=10
    start
    ```
    An emitter never associates and carries no payload meaning, so one emitter
    can sound any number of sniffer collectors at once; unicasting to a specific
    collector's MAC tends to raise that collector's CSI callback rate.

## Important Notes

> 💡 SSIDs and passwords with spaces can be passed as quoted strings to `set-wifi`. Both quote styles work — `--sta-ssid='My WiFi'` and `--sta-ssid="My WiFi"` are equivalent — so you can pick whichever your terminal/keyboard makes easier to type. Underscores (`_`) are passed through literally.

> 🛑 On ESP32-C5, **5 GHz passive sniffer** CSI may return a frozen IQ buffer (driver bug).
> Use the **AP↔STA pair** (`wifi-ap` + `station`) instead.

> 🛑 To stop a running collection early — including indefinite runs started without `--duration` — press `q` (or `Q`) on the serial console.

> ⚡ **Throughput (high-rate CSI):** `set-log-mode --mode=serialized`,
> `set-io-tasks --tx=off` on the station board, and `set-traffic --frequency-hz=4000`
> on the AP. Avoid `set-csi-delivery --mode=callback` during capture
> (it was the old default and caps output around ~10 Hz). The `q` stop key is polled
> by the CLI main loop every 5 ms, not in the WiFi callback.

## Enabling Logging w/ `defmt`

This application can use either the standard `println!` macros or the `defmt` framework for logging. `defmt` produces compact binary frames that the host (`espflash`, `probe-rs`, etc.) decodes against the original ELF, so it's both faster on the device and richer on the host.

The recommended way is the `*-defmt` / `*-defmt-build` cargo aliases — pick the one matching your chip:

```bash
cargo esp32c6-defmt          # build + flash + monitor with defmt decoding
cargo esp32c6-defmt-build    # build only, skip flashing
```

Each defmt alias automatically:
- drops `println` from the default features and enables `defmt`,
- appends `-Tdefmt.x` to the linker script set (so the `.defmt` ELF section is emitted),
- swaps `espflash`'s runner to `espflash flash --monitor --log-format defmt` so log frames are decoded inline.

No edits to `.cargo/config.toml` are required — the aliases pass everything through `cargo --config` overrides at invocation time. If you want to invoke `cargo build` directly (e.g. in CI), the equivalent is:

```bash
cargo build --release \
  --no-default-features \
  --features esp32c6,defmt,no-std,auto,statistics \
  --target riscv32imac-unknown-none-elf \
  --config 'target.riscv32imac-unknown-none-elf.rustflags=["-C", "link-arg=-Tdefmt.x"]'
```

Xtensa targets (`esp32`, `esp32s3`) need the `-Wl,` prefix on the link arg because their toolchain goes through a GCC linker driver:

```bash
--config 'target.xtensa-esp32s3-none-elf.rustflags=["-C", "link-arg=-Wl,-Tdefmt.x"]'
```

## Documentation

- [`specs/WEBSERVER.md`](specs/WEBSERVER.md) — web-server / host-automation integration (REST mapping, pairing presets, v0.7.0 delta)
- [`specs/SPECS.md`](specs/SPECS.md) — complete on-device CLI specification
- [esp-csi-rs on docs.rs](https://docs.rs/esp_csi_rs) — underlying library API

## Development

This crate is still in early development and currently supports `no-std` only. Contributions and suggestions are welcome!

## License
Copyright 2026 The csi-rs Team

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at
http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

---

Made with 🦀 for ESP chips
