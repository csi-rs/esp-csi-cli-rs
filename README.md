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

* **Multiple Wi-Fi Modes:** Configure the ESP device as a Station, Sniffer, ESP-NOW Central, or ESP-NOW Peripheral.
* **Traffic Generation:** Generate traffic at configurable intervals.
* **Fine-grained CSI Control:** Enable or disable specific CSI features like LLTF, HTLTF, STBC HTLTF, and LTF Merge.
* **PHY Rate / IO Task Control:** Pin the ESP-NOW PHY rate and toggle TX or RX direction tasks at the CLI.
* **Runtime Delivery Switching:** Flip CSI delivery between async-queued, inline callback, and off without re-flashing.
* **Statistics Snapshot:** `show-stats` reports PPS, drops, and one/two-way ESP-NOW latency on demand.
* **Collection Mode:** Switch the node between Collector and Listener roles at runtime.
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
    | `esp32c6`     | Target: ESP32-C6 (WiFi 6)                                            |
    | `esp32s3`     | Target: ESP32-S3                                                     |
    | `println`     | Log via `println!` (default)                                         |
    | `defmt`       | Log via `defmt` (efficient binary logging)                           |
    | `auto`        | Auto-select JTAG or UART backend at runtime (default)                |
    | `async-print` | Non-blocking async logging — unstable, use with caution              |
    | `statistics`  | Expose runtime PPS/latency/drop counters via `show-stats` (default)  |
    | `jtag-serial` | Force JTAG serial backend                                            |
    | `uart`        | Force UART backend                                                   |

    ```bash
    # Example: ESP32-C6 with defmt, forced JTAG backend
    cargo build --no-default-features --features "no-std,esp32c6,defmt,jtag-serial" \
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
> 📝 The `set-csi` command options differ on the ESP32-C5 and ESP32-C6 (which expose the HE/STBC field set instead of the classic LLTF/HTLTF flags).

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

* **`set-collection-mode [OPTIONS]`**
    * Description: Set the CSI node collection role.
    * Options:
        * `--mode=<collector|listener>`: `collector` actively generates and collects CSI data (default). `listener` passively receives CSI data only.
    * Examples:
        * `set-collection-mode --mode=collector`
        * `set-collection-mode --mode=listener`

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
    * Description: Configure CSI feature flags.
    * Options (ESP32, ESP32-C3, ESP32-S3):
        * `--disable-lltf`: Disable LLTF CSI (default: enabled).
        * `--disable-htltf`: Disable HTLTF CSI (default: enabled).
        * `--disable-stbc-htltf`: Disable STBC HTLTF CSI (default: enabled).
        * `--disable-ltf-merge`: Disable LTF Merge CSI (default: enabled).
    * Options (ESP32-C5, ESP32-C6):
        * `--disable-csi`: Disable acquisition of CSI entirely.
        * `--disable-csi-legacy`: Disable L-LTF acquisition for 11g PPDUs.
        * `--disable-csi-ht20`: Disable HT-LTF for HT20 PPDUs.
        * `--disable-csi-ht40`: Disable HT-LTF for HT40 PPDUs.
        * `--disable-csi-su`: Disable HE-LTF for HE20 SU PPDUs.
        * `--disable-csi-mu`: Disable HE-LTF for HE20 MU PPDUs.
        * `--disable-csi-dcm`: Disable HE-LTF for HE20 DCM PPDUs.
        * `--disable-csi-beamformed`: Disable HE-LTF for HE20 Beamformed PPDUs.
        * `--csi-he-stbc=<0-2>`: STBC HE LTF selection (default: 2).
        * `--val-scale-cfg=<0-3>`: Value scale configuration (default: 2).
    * Examples:
        * `set-csi --disable-lltf --disable-ltf-merge`
        * `set-csi --disable-csi-legacy --csi-he-stbc=1`

* **`set-wifi [OPTIONS]`**
    * Description: Configure WiFi and network settings. **Note:** SSIDs/passwords with spaces should be wrapped in single or double quotes (e.g. `--sta-ssid='My Network'` or `--sta-ssid="My Network"`). Both quote styles are interchangeable. Underscores (`_`) are passed through literally.
    * Options:
        * `--mode=<station|sniffer|esp-now-central|esp-now-peripheral>`: Specify WiFi operation mode (default: `sniffer`).
        * `--sta-ssid=<SSID>`: Set the SSID for Station mode.
        * `--sta-password=<PASSWORD>`: Set the password for Station mode.
        * `--set-channel=<NUMBER>`: Set the WiFi channel (default: 1).
    * Examples:
        * `set-wifi --mode=sniffer --set-channel=6`
        * `set-wifi --mode=station --sta-ssid="My Network" --sta-password="my password"`
        * `set-wifi --mode=esp-now-central`

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

* **`set-rate [OPTIONS]`** *(ESP-NOW only)*
    * Description: Pin the Wi-Fi PHY rate used by ESP-NOW central / peripheral nodes. Sniffer and station modes ignore this and derive their rate from the surrounding radio configuration.
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
        * `set-io-tasks --tx=off`         (listener-only)
        * `set-io-tasks --tx=on --rx=on`  (default)

* **`set-csi-delivery [OPTIONS]`**
    * Description: Switch the CSI delivery mode at runtime, and independently toggle the inline UART/JTAG log gate. The two delivery paths are mutually exclusive — the WiFi callback only ever pays for one per packet.
    * Options:
        * `--mode=<off|callback|async>`: `off` drops user delivery, `callback` invokes the registered `set_csi_callback` hook inline in the WiFi callback, `async` queues to `CSINodeClient::next_csi_packet` (default for the indefinite collection path).
        * `--logging=<on|off>`: Toggle the per-packet `log_csi` UART/JTAG gate independently.
    * Examples:
        * `set-csi-delivery --mode=async`
        * `set-csi-delivery --mode=off --logging=off`

* **`show-stats`** *(requires `statistics` feature, on by default)*
    * Description: Print a one-shot snapshot of runtime CSI / traffic counters: RX/TX packet totals, average PPS, RX/TX rate in Hz, RX dropped packets, one-way and two-way ESP-NOW latency. Counters reset on the start of each new `start` collection.
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

3.  **Configure an ESP as an ESP-NOW Central node in listener mode and collect for 2 minutes:**
    ```
    set-wifi --mode=esp-now-central
    set-collection-mode --mode=listener
    set-log-mode --mode=array-list
    show-config
    start --duration=120
    ```

4.  **ESP-NOW pair: pin the PHY rate and disable TX on the listener node, then check stats mid-run:**
    ```
    set-wifi --mode=esp-now-peripheral
    set-rate --rate=mcs0-lgi
    set-io-tasks --tx=off
    set-csi-delivery --mode=async --logging=on
    start
    # ... in another window or after pressing 'q' to stop:
    show-stats
    ```

5.  **Emit ESP32-CSI-Tool-compatible CSV for a host pipeline:**
    ```
    set-wifi --mode=sniffer --set-channel=6
    set-log-mode --mode=esp-csi-tool
    start --duration=60
    ```

## Important Notes

> 💡 SSIDs and passwords with spaces can be passed as quoted strings to `set-wifi`. Both quote styles work — `--sta-ssid='My WiFi'` and `--sta-ssid="My WiFi"` are equivalent — so you can pick whichever your terminal/keyboard makes easier to type. Underscores (`_`) are passed through literally.

> 🛑 To stop a running collection early — including indefinite runs started without `--duration` — press `q` (or `Q`) on the serial console.

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

This CLI is built around the esp-csi-rs crate. You can find full documentation for esp-csi-rs on [docs.rs](https://docs.rs/esp_csi_rs).

## Development

This crate is still in early development and currently supports `no-std` only. Contributions and suggestions are welcome!

## License
Copyright 2026 The esp-csi Team

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
