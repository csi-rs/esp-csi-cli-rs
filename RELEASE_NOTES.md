# Release Notes

## v0.8.0

Highlights: the CLI moves to the **esp-csi-rs 0.11 node model**, adds the emitter
modes next to the ESP-NOW pairs, and gains a `--collection` setting, CSI filters
and a firmware `version` command.

### Migrated to the esp-csi-rs 0.11 node model (BREAKING)

esp-csi-rs 0.11 describes every node by four attributes — network role, collection
mode, operational mode and session role — defined in the
[node model](https://github.com/csi-rs/esp-csi-rs/blob/main/docs/network-model.md).
The CLI now builds each run from an `OperationalMode` instead of the old role type.
Every existing `--mode` string is kept: that grammar is the serial contract shared
with the webserver, web client and desktop GUI.

- **Nine `set-wifi --mode=` values.** `station`, `sniffer`, `wifi-ap`,
  `ht20-emitter`, `ht40-emitter`, `esp-now-central`, `esp-now-peripheral`,
  `esp-now-fast-collector` and `esp-now-fast-source`. The ESP-NOW modes are
  retained next to the new emitters, not replaced by them.
- **Simplex aliases.** The simplex ends also accept the model spellings:
  `esp-now-simplex-peer` is `esp-now-fast-collector`, and
  `esp-now-simplex-source` is `esp-now-fast-source`. The `-fast-` strings keep
  working.
- **The simplex ends swapped sides.** The source end, which floods, sources the
  traffic and is now the **central listener**. The peer end, which beacons and then
  only receives, is now the **peripheral collector**. The `--mode` strings and the
  on-air behaviour are unchanged; only the network role each end reports moved.
- **New emitter modes.** `ht20-emitter` / `ht40-emitter` bring up an unassociated
  STA at a forced HT TX PHY and loop-inject a rate-agnostic sounding frame (20 MHz,
  or 40 MHz with the secondary above the primary). An emitter is a central listener:
  TX only, it never captures CSI. Configure it with `--set-channel`, `--peer-mac`
  (injection destination, default broadcast) and `--inject-period-us`. Pair it with
  a `sniffer`.
- **`set-collection-mode` is gone; its two jobs are now two commands.**
  - `set-wifi --collection=collector|listener` sets the node's **collection mode**
    (default `collector`). A listener captures CSI but reports none. Only the modes
    that admit a choice read it: `esp-now-central`, `esp-now-peripheral`, `station`
    and `wifi-ap`. A sniffer and the simplex peer are always collectors; an emitter
    and the simplex source are always listeners. On the ESP-NOW modes the setting is
    also announced to the peer.
  - `set-csi-output --enabled=true|false` is the **runtime delivery gate**
    (default `true`), mapping to `esp_csi_rs::set_csi_output_enabled`. With delivery
    off the radio still captures, with the RX path and its timing unchanged, but
    nothing is decoded, logged or handed to a callback. It has no effect on an
    emitter.
- **`set-csi-output` now actually gates delivery.** It used to call the `CSINode`
  method, which wrote a flag no CSI path read, so `--enabled=false` reported success
  and kept delivering. It now calls the free function, which closes the gate.
- **`show-config` and the `set-wifi` confirmation print the collection mode**,
  marked `fixed by mode` where the mode does not read `--collection`.
- **`--peer-mac` / `--ht40` are read per mode.** `--peer-mac` is the emitter's
  injection destination (empty = broadcast) or the explicit ESP-NOW peer (empty =
  automatic pairing), to be set on both nodes. `--ht40` is the `wifi-ap` softAP
  secondary channel, or the per-peer HT40 TX PHY in the ESP-NOW modes. It never
  selects emitter bandwidth; use `--mode=ht40-emitter` for that.
- **Fix: the simplex peer ignored `--peer-mac`.** Only the source end applied it,
  so an explicitly pinned simplex pair still discovered its peer. Both ends now
  apply it.
- **`set-rate` is reporting only, except on the ESP-NOW pair.**
  `esp-now-central` / `esp-now-peripheral` apply it as the per-peer TX PHY. Every
  other mode stores it and echoes it in `show-config`.
- **`show-stats` drops the ESP-NOW TX queued / confirmed / failed counters.**

### New commands and options

- **`set-csi-filter --peer-mac=<mac|any> --min-phy=<any|ht>`.** A collector is
  promiscuous, so a capture mixes the AP's beacons and ACKs and third-party devices
  in with the configured traffic. The filter drops rejected frames in the Wi-Fi
  callback, before the packet copy and before formatting, and counts them as RX
  drops in `show-stats`.
- **`set-wifi --inject-period-us=<US>`** (default 20000 us, about 50 frames/s).
  Whole milliseconds only express `1000/n` Hz, so above about 250 Hz the old
  `--inject-period-ms` had no usable control left. The ms flag still works, and
  when both are sent the µs value wins.
- **`set-wifi --emitter-iface=sta|ap`** picks the interface an emitter injects on.
  Which one actually radiates is chip-dependent, so the choice is the operator's.
- **`version`** prints `open <semver>`, so host tooling can detect the build flavor
  from a positive statement instead of probing a mode string.
- **Build-time console baud.** `ESP_CSI_CLI_UART_BAUD` (default 115200) is read in
  `build.rs` and applied to the UART console. It is fixed per image and declared:
  `info` reports `baud=`, and the release manifest carries it per asset.
- **`info` declares `log=` and `transport=`.** A `defmt` build emits binary frames
  on the same line a host parses as CSI text, so the host needs the device to say
  which decoder to use.
- `restart` and `set-traffic --unsolicited=<on|off>` were already in v0.7.0 but
  missing from its notes. Both are now documented in `docs/commands.md`.
- `CLI_PROTOCOL_VERSION` stays at **2**. `info` gained lines but lost none, and host
  parsers ignore unknown keys.

### Wi-Fi access point

- **Synchronized burst flood (`set-wifi --ap-burst=<on|off>`)** integrates
  esp-csi-rs's sync burst TX mode for `wifi-ap`. With `--ap-burst=on`, every flood
  tick sends one unicast frame back-to-back to **all** associated stations instead
  of round-robining one station per tick, so all stations capture their downlink
  CSI within tens of microseconds of each other (time-aligned multi-receiver
  capture). Each station then sees the full `frequency-hz` rate, so total offered
  airtime is `frequency-hz × leases`; lower `set-traffic --frequency-hz` if the
  channel saturates. Default is `off` (round-robin, rate shared). `show-config` and
  the `set-wifi` confirmation block report `AP Burst`.
- **Multi-station `wifi-ap` fix.** The softAP DHCP pool was left at the
  `WifiApConfig` default of **1 lease**, so with two or more stations only the
  first got an IP and the ICMP flood targeted that single address; every other
  station captured **zero** CSI. The CLI now configures a **4-lease pool by
  default**, which switches the flood to the round-robin multi-target path.
  New `set-wifi --ap-leases=<1-8>` sizes the pool; `1` restores the legacy
  single-target flood. `show-config` and the `set-wifi` confirmation block report
  `AP Leases`.

### Dependencies

- Requires **esp-csi-rs 0.11**. Until 0.11 is on crates.io the build resolves it
  through the `[patch.crates-io]` path entry in `Cargo.toml`
  (`esp-csi-rs = { path = "../esp-csi-rs" }`), which is dropped once it publishes.

## v0.7.0

Highlights: three new Wi-Fi operating modes, and full softAP configuration at
the CLI.

### CSI collection (ESP32-C5 / ESP32-C6)

- **ESP32-C5 default channel** is now **149** (5 GHz); other chips remain on ch1.
- **C5-only CSI toggles**: `--csi-force-lltf`, `--csi-vht`, `--dump-ack`.
- See README example 7 for a full AP↔STA pair.
- **Station band hint (C5)** — `set-wifi --set-channel=<ch>` is forwarded as
  `WifiStationConfig::channel_hint` so dual-band C5 stations select 2.4 vs
  5 GHz before association.
- **Per-chip heap** — reclaimed-RAM allocator sized to each target's link
  budget: **65 KiB** (C3/C5/C6), **72 KiB** (S3), **~96 KiB** (ESP32). Needed
  for association headroom beyond the old 60 KiB default.
- **CSI throughput fix** — collection no longer registers a per-packet
  `set_csi_callback` (clone + JTAG FIFO drain capped output at ~10 Hz). Output
  now uses the same inline `log_csi` path as the esp-csi-rs examples; `q`
  stop is polled from the CLI loop every 5 ms.

### New `set-wifi` modes

- **`wifi-ap`** — self-contained softAP CSI collector (DHCP + ICMP flood to
  leased client). Pair with `station` on the same SSID for bidirectional lab
  traffic.
- **`esp-now-fast-collector`** — asymmetric ESP-NOW simplex collector (sparse
  beacon, then RX-only after source detection).
- **`esp-now-fast-source`** — matching fast ESP-NOW source (unicast flood at
  forced PHY).

### New AP CLI options

- `--ap-ssid=<SSID>` — softAP SSID (default: `esp-csi-ap`).
- `--ap-password=<PASSWORD>` — WPA2 password; empty = open network.
- `--ap-dhcp=<on|off>` — enable/disable the built-in single-lease DHCP server
  (default: on).

`--peer-mac` and `--ht40` apply to all ESP-NOW modes, including the fast
simplex pair. For AP + STA lab pairs, use `set-protocol --protocol=n`.
Consider `set-traffic --frequency-hz=4000` (library example rate).

### Dependencies

- Bumped `esp-csi-rs` `0.7.3` → `0.8.1`.

### Docs

- [`specs/WEBSERVER.md`](specs/WEBSERVER.md) — web-server / host-automation
  integration guide (REST mapping, pairing presets, v0.7.0 wire-format delta).
- [`specs/SPECS.md`](specs/SPECS.md) updated for v0.7.0 WiFi modes and AP CLI
  options.