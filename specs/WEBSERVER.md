# esp-csi-cli-rs — Web Server Integration Guide

This document describes how to drive `esp-csi-cli-rs` **v0.8.0** from a host
web server or automation layer over a serial transport (UART or USB-Serial-JTAG).
It focuses on wire formats, the command surface, and multi-device pairing flows.
The vocabulary (operational mode, network role, collection mode) follows the
[esp-csi-rs node model](https://github.com/csi-rs/esp-csi-rs/blob/main/docs/network-model.md).

For the full on-device CLI specification see [`SPECS.md`](SPECS.md).

---

## 1. Transport model

| Aspect | Detail |
|--------|--------|
| Transport | One serial port per board (`/dev/ttyACM*`, `/dev/ttyUSB*`, etc.) |
| Framing | Line-oriented text; commands terminated with `\r` or `\n` |
| Prompt | `> ` when idle; CSI data streams as plain text lines during `start` |
| Device key | Factory MAC (`mac=` in banner / `info`) — **not** the port path |
| Collection lock | While collecting, only `q`/`Q` is accepted (early stop) |

A typical web-server architecture:

```text
Browser/API  →  Web server  →  Serial bridge (one task per MAC)  →  ESP board
                     ↑
              Parse `info`, `show-config`, CSI lines
```

---

## 2. Device discovery & identity

### 2.1 Passive (on reset / reconnect)

After reset the firmware emits a welcome banner. The first lines are:

```text
ESP-CSI-CLI/0.8.0
mac=D0:CF:13:E2:90:E8
******* Welcome to the CSI Collection CLI utility! *******
...
```

**Web server action:** match `^ESP-CSI-CLI/(\d+\.\d+\.\d+)$` on the first line,
then read `mac=([0-9A-F:]{17})` on the next line. Pin the serial session to that
MAC so USB re-enumeration after `restart` re-binds correctly.

### 2.2 Active (`info` command)

Send `info\r`. Response (stable for `protocol=2`):

```text
ESP-CSI-CLI/0.8.0
name=esp-csi-cli-rs
version=0.8.0
chip=esp32c6
protocol=2
mac=D0:CF:13:E2:90:E8
log=text
transport=auto
baud=115200
features=statistics,println,auto
END-INFO
```

| Key | Use |
|-----|-----|
| `version` | Feature gating (e.g. require `>= 0.8.0` for `--collection`) |
| `protocol` | Wire-format version; currently `2` |
| `mac` | Stable device ID for your DB / session map |
| `chip` | UI labels, chip-specific CSI options |
| `log` | `text` or `defmt`: which decoder the log frames need |
| `transport` | `auto`, `jtag` or `uart`: the console the build writes to |
| `baud` | UART rate the build was compiled with |
| `features` | Unordered set; `statistics` ⇒ `show-stats` exists |

Parsers should ignore unknown keys: `info` gains lines without a `protocol` bump.

---

## 3. Mode and option surface

### 3.1 Operational modes (`set-wifi --mode=`)

Each `--mode` value selects an operational mode (how the node reaches the
channel) and, where the link has two ends, which end. The collection mode is set
separately with `--collection`, and only where the mode admits a choice.

| CLI value | `NodeMode` (in `show-config`) | Operational mode | Link end / collection mode |
|-----------|-------------------------------|------------------|----------------------------|
| `sniffer` | `WifiSniffer` | Wi-Fi sniffer | peripheral; always a collector |
| `station` | `WifiStation` | Wi-Fi station | associates to an existing AP; `--collection` |
| `wifi-ap` | `WifiAccessPoint` | Wi-Fi access point | central softAP (DHCP + ICMP flood); `--collection` |
| `ht20-emitter` | `Ht20Emitter` | Emitter | central; always a listener (20 MHz HT, TX only) |
| `ht40-emitter` | `Ht40Emitter` | Emitter | central; always a listener (40 MHz HT, TX only) |
| `esp-now-central` | `EspNowCentral` | ESP-NOW | central end (sources the traffic); `--collection` |
| `esp-now-peripheral` | `EspNowPeripheral` | ESP-NOW | peripheral end (replies); `--collection` |
| `esp-now-fast-collector` (alias `esp-now-simplex-peer`) | `EspNowFastCollector` | ESP-NOW simplex | peer end: peripheral collector (sparse beacon, then RX-only) |
| `esp-now-fast-source` (alias `esp-now-simplex-source`) | `EspNowFastSource` | ESP-NOW simplex | source end: central listener (unicast flood at forced PHY) |

Both members of an ESP-NOW pair, simplex included, must share `--set-channel`.
Pairing is automatic unless `--peer-mac` is set on **both** nodes. An emitter
needs no pair at all — any number of `sniffer`s on its channel will hear it.

Since v0.8.0 the simplex source is the central and the peer is the peripheral;
the ends were labelled the other way round before. The `--mode` strings and the
on-air behaviour did not change.

### 3.2 `set-wifi` options

| Argument | Values | Default | Applies to |
|----------|--------|---------|------------|
| `--ap-ssid=<SSID>` | ≤ 32 bytes; quote for spaces | `esp-csi-ap` | `wifi-ap` |
| `--ap-password=<PASSWORD>` | ≤ 32 bytes; empty = open | *(empty)* | `wifi-ap` |
| `--ap-dhcp=<on\|off>` | `on`, `off`, `true`, `false`, `1`, `0`, `yes`, `no` | `on` | `wifi-ap` |
| `--ap-leases=<1-8>` | DHCP lease pool size; > 1 round-robins the ICMP flood across all associated stations | `4` | `wifi-ap` |
| `--inject-period-us=<US>` | Emitter inter-frame period (≈ 20000 us → 50 fps); wins over the ms flag | `20000` | `ht*-emitter` |
| `--inject-period-ms=<MS>` | Emitter inter-frame period in whole ms (coarse) | — | `ht*-emitter` |
| `--emitter-iface=<sta\|ap>` | Which interface injects | `sta` | `ht*-emitter` |
| `--collection=<collector\|listener>` | Collection mode; a listener captures but reports nothing | `collector` | `station`, `wifi-ap`, `esp-now-central`, `esp-now-peripheral` (ignored elsewhere; the mode fixes it) |

Existing options: `--sta-ssid`, `--sta-password`, `--set-channel` as before.
`--peer-mac` is read per mode — the emitter's injection destination (empty =
broadcast) or the explicit ESP-NOW peer, simplex included (empty = automatic pairing). `--ht40` is
the softAP secondary channel in `wifi-ap` or the per-peer HT40 TX PHY in the
ESP-NOW modes; it never selects emitter bandwidth, which is `--mode=ht40-emitter`.

### 3.3 `UserConfig` fields (defaults)

| Field | Default |
|-------|---------|
| `ap_ssid` | `esp-csi-ap` |
| `ap_password` | *(empty — open AP)* |
| `serve_dhcp` | `true` |
| `ap_lease_count` | `4` |
| `csi_output_enabled` | `true` |
| `collection_collector` | `true` (collector) |
| `inject_period_us` | `20000` |

### 3.4 `show-config` additions

The `[WiFi]` section now always includes:

```text
  AP SSID : 'esp-csi-ap'
  AP Pass : open          # or  AP Pass : 'secret'
  AP DHCP : true
  AP Leases: 4
  AP Burst : false
```

The `[Collection]` section starts with the collection mode the next run uses:

```text
  Collection    : collector   # or  listener, or  collector|listener (fixed by mode)
```

### 3.5 `set-wifi` confirmation output

After `set-wifi`, the device prints an **Access Point Settings** block:

```text
Updated WiFi Configuration:

WiFi Mode: WifiAccessPoint
WiFi Channel: 6
Station WiFi Settings:
SSID: '', Password: ''
Access Point Settings:
SSID: 'esp-csi-ap', Password: (open), DHCP: true, Leases: 4, Burst: false
Peer MAC: unset (emitter broadcasts / ESP-NOW auto-pairs)
Secondary Channel: HT20/legacy
Emitter Period: 20000us
Collection: collector
```

### 3.6 Behavior notes for integrators

- **Collection mode vs. output gate:** the old
  `set-collection-mode --mode=collector|listener` is gone. The node's collection
  mode is now `set-wifi --collection=collector|listener`, read only by `station`,
  `wifi-ap`, `esp-now-central` and `esp-now-peripheral` (the ESP-NOW modes also
  announce it to the peer). `set-csi-output --enabled=<true|false>` is the
  runtime delivery gate: with delivery off the radio still captures (RX path and
  timing unchanged) but nothing is decoded or logged.
- **Power saving:** AP and station modes disable Wi-Fi power saving at collection
  start (throughput-oriented).
- **`set-rate`:** Reporting only, except on the ESP-NOW pair:
  `esp-now-central` / `esp-now-peripheral` apply it as the per-peer TX PHY.
  Station, sniffer and wifi-ap take their rate from the surrounding radio
  configuration, an emitter's rate follows its forced TX PHY, and the simplex
  source uses its profile's fixed rate.
- **`set-protocol`:** User-configurable (`lr` default). For AP + STA lab pairs
  use `n` on **both** boards.
- **`set-traffic`:** Default remains 100 Hz. Library AP/STA examples use 4000 Hz;
  expose this as a “high throughput” preset in your UI.

---

## 4. Command reference (web-server mapping)

Commands are sent as a single line (no shell). Example: `set-wifi --mode=wifi-ap --set-channel=6\r`

### 4.1 Configuration commands (apply on next `start`)

| CLI command | Typical web API | Notes |
|-------------|-----------------|-------|
| `set-wifi …` | `POST /devices/{mac}/wifi` | Mode, channel, STA/AP/emitter/ESP-NOW fields |
| `set-traffic --frequency-hz=N` | `POST /devices/{mac}/traffic` | `0` = off |
| `set-csi-output --enabled=true\|false` | `POST /devices/{mac}/csi-output` | Default `true` |
| `set-protocol --protocol=n` | `POST /devices/{mac}/protocol` | `b\|g\|n\|lr\|a\|ac` |
| `set-rate --rate=mcs7-lgi` | `POST /devices/{mac}/rate` | Reporting only, except on the ESP-NOW pair |
| `set-csi-filter --peer-mac=<mac\|any> --min-phy=<any\|ht>` | `POST /devices/{mac}/csi-filter` | Drop frames before delivery |
| `set-io-tasks --tx=on --rx=on` | `POST /devices/{mac}/io-tasks` | |
| `set-csi …` | `POST /devices/{mac}/csi` | Chip-specific flags |
| `reset-config` | `POST /devices/{mac}/reset-config` | Restore all defaults |

### 4.2 Immediate commands

| CLI command | Typical web API | Notes |
|-------------|-----------------|-------|
| `set-log-mode --mode=array-list` | `POST /devices/{mac}/log-mode` | Takes effect on next packet |
| `set-csi-delivery --mode=callback` | `POST /devices/{mac}/csi-delivery` | Overridden partially at `start` |
| `info` | `GET /devices/{mac}/info` | Machine-parseable block |
| `version` | `GET /devices/{mac}/version` | `open <semver>` |
| `show-config` | `GET /devices/{mac}/config` | Human-readable; parse lines or regex |
| `show-stats` | `GET /devices/{mac}/stats` | Requires `statistics` feature |
| `restart` | `POST /devices/{mac}/restart` | Not available during collection |

### 4.3 Collection lifecycle

| CLI command | Typical web API | Response / side effects |
|-------------|-----------------|-------------------------|
| `start` | `POST /devices/{mac}/start` | `Starting CSI collection indefinitely...`; CLI locks |
| `start --duration=120` | `POST /devices/{mac}/start?duration=120` | Timed run |
| `q` (during run) | `POST /devices/{mac}/stop` | `Stopping...` then `Collection complete.` |

During `start`, CSI packets are written as log lines (format set by `set-log-mode`).

---

## 5. Parsing `show-config`

Recommended approach: send `show-config\r`, read until the closing
`===================================` line.

### 5.1 `[WiFi]` section

```text
[WiFi]
  Mode    : WifiAccessPoint
  Channel : 6
  STA SSID: 'my-router'
  STA Pass: 'secret'
  AP SSID : 'esp-csi-ap'
  AP Pass : open
  AP DHCP : true
  Dst MAC : aa:bb:cc:dd:ee:ff   # or  broadcast
  AP 2nd  : HT20/legacy         # or  HT40 (secondary above|below)
```

**Suggested regex keys:**

| Line prefix | Capture |
|-------------|---------|
| `Mode    : ` | `WifiSniffer`, `WifiStation`, `WifiAccessPoint`, `Ht20Emitter`, `Ht40Emitter`, `EspNowCentral`, `EspNowPeripheral`, `EspNowFastCollector`, `EspNowFastSource` |
| `Channel : ` | integer 1–14 |
| `STA SSID: '` | SSID (strip quotes) |
| `AP SSID : '` | SSID (strip quotes) |
| `AP Pass : ` | `open` or quoted password |
| `AP DHCP : ` | `true` / `false` |
| `Dst MAC : ` | MAC or `broadcast` |
| `Collection    : ` | *(in `[Collection]` section)* `collector` or `listener`, optionally followed by `(fixed by mode)` |
| `Protocol      : ` | *(in `[Collection]` section)* `LR`, `N`, etc. |

### 5.2 Example JSON mapping

```json
{
  "wifi": {
    "mode": "WifiAccessPoint",
    "channel": 6,
    "sta": { "ssid": "", "password": "" },
    "ap": { "ssid": "esp-csi-ap", "password": null, "dhcp": true },
    "dstMac": null,
    "apSecondary": null
  },
  "collection": {
    "csiOutput": true,
    "trafficHz": 100,
    "phyRate": "RateMcs0Lgi",
    "protocol": "LR",
    "ioTasks": { "tx": true, "rx": true }
  }
}
```

---

## 6. Pairing cookbooks (command sequences)

Store these as presets in your web server. Each step is one line sent to the
device serial port; wait for the prompt (`> `) or expected acknowledgment before
the next command unless noted.

### 6.1 SoftAP lab pair (board A = access point, board B = station)

**Board A — access point (central collector)**

```text
reset-config
set-wifi --mode=wifi-ap --set-channel=6 --ap-ssid=esp-csi-ap
set-protocol --protocol=n
set-traffic --frequency-hz=4000
set-log-mode --mode=array-list
show-config
start
```

**Board B — station (match AP SSID and channel)**

```text
reset-config
set-wifi --mode=station --sta-ssid=esp-csi-ap --set-channel=6
set-protocol --protocol=n
set-traffic --frequency-hz=4000
set-log-mode --mode=array-list
show-config
start
```

CSI output appears primarily on **board A** (AP uplink path).

The AP's DHCP pool holds 4 leases by default, and with more than one lease the
ICMP flood round-robins across **all** associated stations — so the pair scales
to multiple station boards without extra configuration. Use
`set-wifi --ap-leases=<1-8>` to size the pool (`1` = legacy single-target
flood). The offered rate is shared: with N stations each sees roughly
`frequency-hz / N` packets per second.

### 6.2 Emitter + sniffer

**Sniffer board** (peripheral collector) — lock the emitter's channel and measure every frame overheard.

```
set-wifi --mode=sniffer --set-channel=6
set-traffic --frequency-hz=0
set-log-mode --mode=array-list
start
```

**Emitter board** (central listener) — 20 MHz; use `ht40-emitter` for 40 MHz bonded.

```
set-wifi --mode=ht20-emitter --set-channel=6 --inject-period-us=20000
start
```

An emitter never associates and its frames carry no payload meaning, so a single
emitter sounds every sniffer in range at once. Unicasting with `--peer-mac` to
one sniffer tends to raise that sniffer's CSI callback rate.

### 6.3 ESP-NOW central + peripheral

Connectionless: no AP, no DHCP, no association. Both ends capture CSI, and each
is a collector unless set to `--collection=listener`.

**Central board** — sources the traffic.

```
set-wifi --mode=esp-now-central --set-channel=6
set-rate --rate=mcs0-lgi
set-log-mode --mode=array-list
start
```

**Peripheral board** — replies on the same channel.

```
set-wifi --mode=esp-now-peripheral --set-channel=6
set-rate --rate=mcs0-lgi
set-log-mode --mode=array-list
start
```

With more than two boards on one channel, set `--peer-mac` on both to pin the
pair explicitly rather than relying on magic-prefix discovery.

### 6.4 ESP-NOW simplex pair (highest CSI rate)

Asymmetric on purpose: the peer (peripheral collector) beacons sparsely until it
hears a source, then stops transmitting and goes RX-only, so all airtime belongs
to the source (central listener). Start the peer first.

```
# Peer board (peripheral collector)
set-wifi --mode=esp-now-simplex-peer --set-channel=6
set-log-mode --mode=serialized
start

# Source board (central listener)
set-wifi --mode=esp-now-simplex-source --set-channel=6
start
```

`esp-now-simplex-peer` / `esp-now-simplex-source` are aliases of
`esp-now-fast-collector` / `esp-now-fast-source`; either spelling works. To pin
the pair, set `--peer-mac` on both boards. `set-rate` does not apply to this
pair — the simplex profile fixes its own PHY.

