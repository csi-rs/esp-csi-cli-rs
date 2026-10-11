# CLI commands

Every command the firmware accepts on the serial console.

## CLI Commands

This is a list of commands available through the CLI interface:
> 📝 The `set-csi` command options differ on the ESP32-C5 and ESP32-C6 (which expose per-PPDU-format acquisition flags — legacy / HT20 / HT40 / HE-LTF, plus VHT20 and forced L-LTF on C5 — instead of the classic LLTF/HTLTF flags).

* **`help [command]`**
    * Description: Display the main help menu or details for a specific command.
    * Example: `help set-wifi`

* **`set-measurement [OPTIONS]`**
    * Description: Configure the node's measurement setup (`esp_csi_rs::MeasurementSetup`).
    * Options:
        * `--periodicity-hz=<NUMBER>`: the measurement periodicity — the rate in Hertz of the sounding traffic this node generates (default: 100): the ESP-NOW initiator's soundings and the responder's reply pacing, the simplex source's flood, the station's gateway ping and the access point's downlink flood. Set to `0` to disable traffic generation (a station then has no session role). The sniffer and the simplex peer sound nothing and ignore it.
        * `--unsolicited=<on|off>`: Flood unsolicited ICMP echo **replies** instead of echo requests (default: `off`). The peer ignores unsolicited replies, so the traffic is strictly one-directional — no reply contention and a stable offered rate, but this node gets no CSI back. Use it on a flooding AP whose paired station reports; keep it off when this node needs CSI from the peer's replies (for example a station flooding a router).
    * Examples:
        * `set-measurement --periodicity-hz=10`
        * `set-measurement --periodicity-hz=0`
        * `set-measurement --periodicity-hz=1000 --unsolicited=on`

* **`set-csi-output [OPTIONS]`**
    * Description: The runtime delivery gate (`esp_csi_rs::set_csi_output_enabled`). With delivery off the radio still captures — RX path and timing unchanged — but nothing is decoded, logged, or handed to a callback. No effect on an emitter, which captures nothing. This is not the node's reporting: to make a node report nothing, use `set-wifi --reporting=none`.
    * Options:
        * `--enabled=<true|false>`: Deliver captured CSI (default: `true`).
    * Examples:
        * `set-csi-output --enabled=true`
        * `set-csi-output --enabled=false`

* **`set-csi-filter [OPTIONS]`**
    * Description: Restrict which captured frames are delivered, by source MAC and/or PHY class. A
      reporting receiver is promiscuous — it reports CSI for every frame its radio decodes, including your
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
      copy and before any formatting. Rejected frames are counted as `RX Filtered` in `show-stats`, not as drops.

* **`set-log-mode [OPTIONS]`**
    * Description: Set the CSI output logging format at runtime.
    * Options:
        * `--mode=<text|array-list|serialized|esp-csi-tool>`: Output format for CSI packets (default: `text`).
            * `text`: Verbose human-readable output with full metadata.
            * `array-list`: Compact CSV-style array, one line per packet — best for host-side data processing.
            * `serialized`: Binary COBS-framed postcard frames in the versioned `esp_csi_rs::wire` format introduced by esp-csi-rs 0.12 and unchanged in 0.13 — most compact. Decode with `esp_csi_rs::wire::decode_cobs`; a decoder written for 0.11 cannot read it.
            * `esp-csi-tool`: Hernandez-style 26-column CSV (`CSI_DATA,...` lines) compatible with the ESP32-CSI-Tool.
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
        * `--preset=<default|he20>`: Apply a CSI acquisition preset. `he20` acquires HE-LTF only (legacy, HT and ACK acquisition off) — use it on the receivers of an HE20 link.
        * `--dump-ack=<on|off>`: Dump 802.11 ACK frames (default: on).
        * `--csi-su`, `--csi-mu`, `--csi-dcm`, `--csi-beamformed`, `--csi-he-stbc` `=<on|off>`: HE-LTF acquisition per HE PPDU kind.
        * `--csi-force-lltf=<on|off>`: Force L-LTF acquisition (ESP32-C5 only).
        * `--csi-vht=<on|off>`: VHT-LTF for VHT20 PPDUs (ESP32-C5 only).
    * Examples:
        * `set-csi --lltf=off --ltf-merge=off`
        * `set-csi --csi-legacy=off --preset=default`
        * `set-csi --csi-ht40=on --csi-ht20=off`

* **`set-wifi [OPTIONS]`**
    * Description: Configure WiFi and network settings. **Note:** SSIDs/passwords with spaces should be wrapped in single or double quotes (e.g. `--sta-ssid='My Network'` or `--sta-ssid="My Network"`). Both quote styles are interchangeable. Underscores (`_`) are passed through literally.
    * Options:
        * `--mode=<station|sniffer|wifi-ap|ht20-emitter|ht40-emitter|esp-now-initiator|esp-now-responder|esp-now-simplex-source|esp-now-simplex-peer>`: the node's mode — how it reaches the channel (default: `sniffer`). On the ESP-NOW pairs the token also picks the session role: the initiator sources the traffic, the responder answers it.
        * `--reporting=<report|none|threshold|decimate>`: the node's **reporting** —
          whether, and how often, its measurements leave it (default: `report`). `none`
          captures and reports nothing; `threshold` (IEEE 802.11bf threshold-based reporting)
          reports only while the channel moves and `decimate` every n-th measurement, with the
          parameters set by `set-reporting`.
          This is not `set-csi-output`, which is a runtime delivery gate: on the ESP-NOW modes the
          reporting also goes out on the wire, which is what lets a responder paired with an
          initiator that reports none promote itself. `station`, `wifi-ap`, `esp-now-initiator` and
          `esp-now-responder` take any value; a sniffer takes all but `none` (it is refused with an
          error). An emitter always reports none and each simplex end is fixed by which end it is,
          so the flag is ignored there rather than silently believed.
        * `--report-to=<host|local>`: where `report` sends measurements (default: `host`). `local`
          delivers them only to on-device consumers (callback / queue), never to the host output.
        * `--he20=<on|off>` *(ESP32-C5/C6 only)*: force HE20 (802.11ax SU, 20 MHz). On
          `ht20-emitter` / `ht40-emitter` it replaces the HT PHY; on `esp-now-initiator` /
          `esp-now-responder` it overrides `--ht40`. Pair the receivers with `set-csi --preset=he20`.
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
          (time-aligned multi-receiver capture). Every station sees the full `periodicity-hz`, so
          total offered airtime is `periodicity-hz × leases` — lower the rate if the channel
          saturates. `off` keeps the round-robin flood (rate shared across stations).
        * `--set-channel=<NUMBER>`: Set the WiFi channel. Use 1–14 on 2.4 GHz; on ESP32-C5
          use 5 GHz channels such as 149 (default: 1 on most chips,
          149 on ESP32-C5).
        * `--peer-mac=<aa:bb:cc:dd:ee:ff>`: One field, two meanings by mode. **Emitter modes** — destination address of injected frames; empty = broadcast (default), and unicasting to a receiver usually raises that receiver's CSI rate. **ESP-NOW modes, simplex included** — explicit peer address; empty keeps automatic magic-prefix pairing, and setting it switches to source-MAC filtering, which requires **both** nodes to be configured with the other's address (use it when more than two boards share a channel).
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
        * `set-wifi --mode=esp-now-initiator --set-channel=6`
        * `set-wifi --mode=esp-now-responder --set-channel=6`
        * `set-wifi --mode=esp-now-simplex-source --set-channel=6`
        * `set-wifi --mode=esp-now-simplex-peer --set-channel=6 --peer-mac=aa:bb:cc:dd:ee:ff`
        * `set-wifi --mode=station --sta-ssid=MyAP --reporting=none`
        * `set-wifi --mode=ht20-emitter --set-channel=6 --he20=on` (C5/C6)

* **`set-reporting [OPTIONS]`**
    * Description: Set the node's reporting and its parameters — the same setting as `set-wifi --reporting=`, plus the destination, threshold and decimation parameters. Parameters are kept when another value is active and apply at once when the matching value already is.
    * Options:
        * `--policy=<report|none|threshold|decimate>`: the reporting. A sniffer refuses `none`.
        * `--report-to=<host|local>`: where `report` sends measurements (default: `host`).
        * `--level=<0-65535>`: threshold score at or above which a measurement is reported (default: `6000`, uncalibrated). A still room does not score zero — two ESP32-C5s at HE20 measured a ~2700 median and ~4300 99th percentile — so calibrate it on your own link.
        * `--hold-ms=<MS>`: how long to keep reporting after a measurement crosses `level` (default: `1000`).
        * `--n=<N>`: decimation factor, report every n-th measurement (default: `2`).
    * Examples:
        * `set-reporting --policy=threshold --level=5000 --hold-ms=500`
        * `set-reporting --policy=decimate --n=10`
        * `set-reporting --policy=report --report-to=local`

* **`set-session [OPTIONS]`**
    * Description: Name the measurement session (`esp_csi_rs::set_session`). The id is stamped on every serialized frame's envelope from now on, across runs, until changed or the board resets (`reset-config` does not clear it). Without it each run draws a random id.
    * Options:
        * `--id=<u32>`: session id, decimal or `0x`-prefixed hex. Required.
        * `--epoch=<unix_us>`: the current UNIX time in microseconds. The next run's first frame announces it, so a host can put every frame on wall time.
    * Examples:
        * `set-session --id=42`
        * `set-session --id=0x1a2b --epoch=1791234567000000`

* **`start [OPTIONS]`**
    * Description: Start the CSI collection process. Ensure the device is configured first. Press `q` (or `Q`) on the serial console at any time to stop collection early.
    * Options:
        * `--duration=<SECONDS>`: Specify the duration in seconds. If omitted, collection runs indefinitely.
    * Examples:
        * `start`
        * `start --duration=120`

* **`show-config`**
    * Description: Display the current configuration settings for all parameters. The `[Node]` block reads the next run's attributes back from the mode it will be built with:
        ```
        [Node]
          Mode: esp-now-initiator
          Session role: initiator
          Sensing role: transmitter-receiver (fixed by mode)
          Reporting: report (host)
          Periodicity: 100 Hz
        ```
        `Session role` is `initiator`, `responder` or `none (observer)`; `Sensing role` is `transmitter`, `receiver` or `transmitter-receiver`; `Reporting` is `report (host)`, `report (local)`, `none`, `threshold (level=…, hold=…ms)` or `decimate N`. Values the mode does not let you choose are marked `(fixed by mode)`. The `[WiFi]`, `[Measurement]` and `[CSI Config]` sections follow.
    * Example: `show-config`

* **`reset-config`**
    * Description: Reset all configurations to their default values.
    * Example: `reset-config`

* **`set-rate [OPTIONS]`** *(reporting only, except on the ESP-NOW pair)*
    * Description: Set the Wi-Fi PHY rate. The `esp-now-initiator` / `esp-now-responder` pair applies it as the per-peer TX PHY. Every other mode only stores it and echoes it in `show-config`: station, sniffer and wifi-ap take their rate from the surrounding radio configuration, an emitter transmits at the rate its forced TX PHY implies, and the ESP-NOW simplex source takes its rate from its own profile.
    * Options:
        * `--rate=<NAME>`: One of `mcs0-lgi` (default), `mcs1-lgi`..`mcs7-lgi`, `mcs0-sgi`, `1m`, `2m`, `5m5`, `11m`, `6m`, `9m`, `12m`, `18m`, `24m`, `36m`, `48m`, `54m`.
    * Examples:
        * `set-rate --rate=mcs0-lgi`
        * `set-rate --rate=24m`

* **`set-protocol [OPTIONS]`**
    * Description: Set the Wi-Fi PHY protocol, applied to the node at the start of each collection run. Pick it to match the link: `lr` for maximum range between ESP devices, `n` when associating to a standard AP in station mode. Not every chip supports every protocol; unsupported values may be rejected by the radio at start.
    * Options:
        * `--protocol=<b|g|n|lr|a|ac|ax>`: The protocol (default: `lr`). `ax` (802.11ax) is ESP32-C5/C6 only; esp-csi-rs then runs the HE20 bring-up for `station`, `wifi-ap` and `sniffer`.
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
        * `--mode=<off|callback|async|raw>`: `off` drops user delivery, `callback` invokes the registered `set_csi_callback` hook inline in the WiFi callback, `async` queues to `CsiNodeClient::next_csi_packet` (default for the indefinite collection path). `raw` is the zero-copy CPU-benchmark fast path: the WiFi callback returns before building the `CsiPacket`, so no CSI is delivered or logged. It applies on the next `start`.
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
    * Fields: `protocol` bumps on any breaking change to this grammar or to the serialized CSI format (currently `4`: the IEEE 802.11bf command vocabulary — `--reporting`, `set-measurement`, the initiator/responder mode tokens; the serialized `wire` format is unchanged from `3`). `mac` is the factory base MAC, stable across restarts, which host tooling pins a device to. `log` is the encoding of log frames and `transport` the console the build writes to, both fixed by the build. `baud` is the UART rate the build was compiled with.
    * Example: `info`

* **`version`**
    * Description: Print one line, the build flavor followed by the firmware version: `open <semver>` for this build. Host tooling reads the flavor from this positive statement rather than probing for mode strings.
    * Example: `version`

* **`restart`**
    * Description: Reboot the chip through a clean software reset: the radio is shut down first, then the chip resets. On native-USB boards the USB device re-enumerates, so the serial port can return as a different `/dev/ttyACM*` node; pin devices by the `mac=` field of `info` rather than by port path. The welcome banner (magic line and `mac=`) is re-emitted on boot.
    * Example: `restart`

* **`show-stats`** *(requires `statistics` feature, on by default)*
    * Description: Print a one-shot snapshot of runtime CSI / traffic counters: RX/TX packet totals, average PPS, RX/TX rate in Hz, and RX dropped packets — losses only, broken down into oversize, queue full and on-air sequence gaps — plus the deliberately withheld frames (`RX Filtered` by `set-csi-filter`, `RX Policy Supp.` by a threshold/decimate policy) and the log lines the logger dropped. Counters reset on the start of each new `start` collection.
    * Example: `show-stats`
