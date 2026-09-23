# CLI commands

Every command the firmware accepts on the serial console.

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
        * `--unsolicited=<on|off>`: Flood unsolicited ICMP echo **replies** instead of echo requests (default: `off`). The peer ignores unsolicited replies, so the traffic is strictly one-directional — no reply contention and a stable offered rate, but this node gets no CSI back. Use it on a flooding AP whose paired station collects; keep it off when this node needs CSI from the peer's replies (for example a station flooding a router).
    * Examples:
        * `set-traffic --frequency-hz=10`
        * `set-traffic --frequency-hz=0`
        * `set-traffic --frequency-hz=1000 --unsolicited=on`

* **`set-csi-output [OPTIONS]`**
    * Description: The runtime delivery gate (`esp_csi_rs::set_csi_output_enabled`). With delivery off the radio still captures — RX path and timing unchanged — but nothing is decoded, logged, or handed to a callback. No effect on an emitter, which captures nothing. This is not the node's collection mode: to make a node a listener, use `set-wifi --collection=listener`.
    * Options:
        * `--enabled=<true|false>`: Deliver captured CSI (default: `true`).
    * Examples:
        * `set-csi-output --enabled=true`
        * `set-csi-output --enabled=false`

* **`set-csi-filter [OPTIONS]`**
    * Description: Restrict which captured frames are delivered, by source MAC and/or PHY class. A
      collector is promiscuous — it reports CSI for every frame its radio decodes, including your
      AP's beacons and ACKs and any third-party device on the channel. See
      [What the first rows of a collection are](collection-format.md#what-the-first-rows-of-a-collection-are).
    * Options:
        * `--peer-mac=<aa:bb:cc:dd:ee:ff|any>`: Deliver CSI only for frames from this source. `any`
          (or an empty value) clears the filter (default: `any`).
        * `--min-phy=<any|ht>`: Minimum PHY class. `ht` keeps 802.11n and better, dropping the
          legacy-rate management/control frames (default: `any`).
    * Examples:
        * `set-csi-filter --peer-mac=aa:bb:cc:dd:ee:ff`
        * `set-csi-filter --min-phy=ht`
        * `set-csi-filter --peer-mac=any --min-phy=any`
    * Note: filtering on the device rather than on the host also returns console bandwidth to the
      traffic you configured — a rejected frame is dropped in the Wi-Fi callback before the packet
      copy and before any formatting. Rejected frames are counted as RX drops in `show-stats`.

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
        * `--mode=<station|sniffer|wifi-ap|ht20-emitter|ht40-emitter|esp-now-central|esp-now-peripheral|esp-now-fast-source|esp-now-fast-collector>`: the node's operational mode — how it reaches the channel (default: `sniffer`). The two simplex ends are also spelled `esp-now-simplex-source` and `esp-now-simplex-peer`; the `-fast-` names remain the wire contract.
        * `--collection=<collector|listener>`: the node's **collection mode** — whether its
          measurements leave it (default: `collector`). A listener captures and reports nothing.
          This is not `set-csi-output`, which is a runtime delivery gate: on the ESP-NOW modes the
          collection mode also goes out on the wire, which is what lets a peripheral paired with a
          listening central promote itself. Only the modes that admit a choice read it —
          `station`, `wifi-ap`, `esp-now-central`, `esp-now-peripheral`. A sniffer is always a
          collector, an emitter always a listener, and each simplex end is fixed by which end it
          is, so the flag is ignored there rather than silently believed.
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
        * `--peer-mac=<aa:bb:cc:dd:ee:ff>`: One field, two meanings by mode. **Emitter modes** — destination address of injected frames; empty = broadcast (default), and unicasting to a collector usually raises that collector's CSI rate. **ESP-NOW modes, simplex included** — explicit peer address; empty keeps automatic magic-prefix pairing, and setting it switches to source-MAC filtering, which requires **both** nodes to be configured with the other's address (use it when more than two boards share a channel).
        * `--ht40=<above|below|none>`: `wifi-ap` mode — softAP secondary channel. ESP-NOW modes — force the per-peer TX PHY to HT40 (default: `none` = HT20). This does **not** pick emitter bandwidth; use `--mode=ht40-emitter` for that.
        * `--inject-period-ms=<MS>`: Emitter modes — delay between injected frames, in whole milliseconds (coarse).
        * `--inject-period-us=<US>`: Emitter modes — delay between injected frames in microseconds (default: `20000` ≈ 50 frames/s). Preferred: whole milliseconds cannot express most rates. It is applied after `--inject-period-ms`, so when both are given this one wins.
        * `--emitter-iface=<sta|ap>`: Emitter modes — which interface injects (default: `sta`).
    * Examples:
        * `set-wifi --mode=sniffer --set-channel=6`
        * `set-wifi --mode=station --sta-ssid="My Network" --sta-password="my password"`
        * `set-wifi --mode=wifi-ap --set-channel=6 --ap-ssid=esp-csi-ap`
        * `set-wifi --mode=ht20-emitter --set-channel=6 --inject-period-ms=20`
        * `set-wifi --mode=ht40-emitter --set-channel=6 --peer-mac=aa:bb:cc:dd:ee:ff`
        * `set-wifi --mode=esp-now-central --set-channel=6`
        * `set-wifi --mode=esp-now-simplex-source --set-channel=6`
        * `set-wifi --mode=esp-now-simplex-peer --set-channel=6 --peer-mac=aa:bb:cc:dd:ee:ff`
        * `set-wifi --mode=station --sta-ssid=MyAP --collection=listener`

* **`start [OPTIONS]`**
    * Description: Start the CSI collection process. Ensure the device is configured first. Press `q` (or `Q`) on the serial console at any time to stop collection early.
    * Options:
        * `--duration=<SECONDS>`: Specify the duration in seconds. If omitted, collection runs indefinitely.
    * Examples:
        * `start`
        * `start --duration=120`

* **`show-config`**
    * Description: Display the current configuration settings for all parameters, including the collection mode the next run will use (marked `fixed by mode` where the mode does not read `--collection`).
    * Example: `show-config`

* **`reset-config`**
    * Description: Reset all configurations to their default values.
    * Example: `reset-config`

* **`set-rate [OPTIONS]`** *(reporting only, except on the ESP-NOW pair)*
    * Description: Set the Wi-Fi PHY rate. The `esp-now-central` / `esp-now-peripheral` pair applies it as the per-peer TX PHY. Every other mode only stores it and echoes it in `show-config`: station, sniffer and wifi-ap take their rate from the surrounding radio configuration, an emitter transmits at the rate its forced TX PHY implies, and the ESP-NOW simplex source takes its rate from its own profile.
    * Options:
        * `--rate=<NAME>`: One of `mcs0-lgi` (default), `mcs1-lgi`..`mcs7-lgi`, `mcs0-sgi`, `1m`, `2m`, `5m5`, `11m`, `6m`, `9m`, `12m`, `18m`, `24m`, `36m`, `48m`, `54m`.
    * Examples:
        * `set-rate --rate=mcs0-lgi`
        * `set-rate --rate=24m`

* **`set-protocol [OPTIONS]`**
    * Description: Set the Wi-Fi PHY protocol, applied to the node at the start of each collection run. Pick it to match the link: `lr` for maximum range between ESP devices, `n` when associating to a standard AP in station mode. Not every chip supports every protocol; unsupported values may be rejected by the radio at start.
    * Options:
        * `--protocol=<b|g|n|lr|a|ac>`: The protocol (default: `lr`).
    * Examples:
        * `set-protocol --protocol=lr`
        * `set-protocol --protocol=n`

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
        * `--mode=<off|callback|async|raw>`: `off` drops user delivery, `callback` invokes the registered `set_csi_callback` hook inline in the WiFi callback, `async` queues to `CSINodeClient::next_csi_packet` (default for the indefinite collection path). `raw` is the zero-copy CPU-benchmark fast path: the WiFi callback returns before building the `CSIDataPacket`, so no CSI is delivered or logged. It applies on the next `start`.
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
        mac=<AA:BB:CC:DD:EE:FF>
        log=<text|defmt>
        transport=<auto|jtag|uart>
        baud=<u32>
        features=<comma-separated-list>
        END-INFO
        ```
    * Fields: `protocol` bumps on any breaking change to this grammar (currently `2`). `mac` is the factory base MAC, stable across restarts, which host tooling pins a device to. `log` is the encoding of log frames and `transport` the console the build writes to, both fixed by the build. `baud` is the UART rate the build was compiled with.
    * Example: `info`

* **`version`**
    * Description: Print one line, the build flavor followed by the firmware version: `open <semver>` for this build. Host tooling reads the flavor from this positive statement rather than probing for mode strings.
    * Example: `version`

* **`restart`**
    * Description: Reboot the chip through a clean software reset: the radio is shut down first, then the chip resets. On native-USB boards the USB device re-enumerates, so the serial port can return as a different `/dev/ttyACM*` node; pin devices by the `mac=` field of `info` rather than by port path. The welcome banner (magic line and `mac=`) is re-emitted on boot.
    * Example: `restart`

* **`show-stats`** *(requires `statistics` feature, on by default)*
    * Description: Print a one-shot snapshot of runtime CSI / traffic counters: RX/TX packet totals, average PPS, RX/TX rate in Hz, and RX dropped packets. Counters reset on the start of each new `start` collection.
    * Example: `show-stats`
