# Release Notes

## Unreleased

- **Migrated to `esp-csi-rs 0.9`** (open facade over `esp-csi-rs-core`).

- **Synchronized burst flood (`set-wifi --ap-burst=<on|off>`)** — integrates
  esp-csi-rs's sync burst TX mode for the wifi-ap collector. With
  `--ap-burst=on`, every flood tick sends one unicast frame back-to-back to
  **all** associated stations instead of round-robining one station per tick,
  so all stations capture their downlink CSI within tens of microseconds of
  each other (time-aligned multi-receiver capture). Each station then
  sees the full `frequency-hz` rate, so total offered airtime is
  `frequency-hz × leases` — lower `set-traffic --frequency-hz` if the channel
  saturates. Default is `off` (round-robin, rate shared). `show-config` and
  the `set-wifi` confirmation block report `AP Burst`.

- **Multi-station `wifi-ap` fix** — the softAP DHCP pool was left at the
  `WifiApConfig` default of **1 lease**, so with two or more stations only the
  first got an IP and the ICMP flood targeted that single address; every other
  station captured **zero** CSI. The CLI now configures a **4-lease pool by
  default**, which switches the flood to the round-robin multi-target path (all
  associated stations get traffic, matching the softAP example's
  `.with_lease_pool(4)`). New `set-wifi --ap-leases=<1-8>` sizes the pool; `1`
  restores the legacy single-target flood. `show-config` and the `set-wifi`
  confirmation block now report `AP Leases`.

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