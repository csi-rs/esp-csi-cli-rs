# esp-csi-cli-rs — Web Server Integration Guide

This document describes how to drive `esp-csi-cli-rs` **v0.7.0** from a host
web server or automation layer over a serial transport (UART or USB-Serial-JTAG).
It focuses on wire formats, new v0.7.0 commands, and multi-device pairing flows.

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
ESP-CSI-CLI/0.7.0
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
ESP-CSI-CLI/0.7.0
name=esp-csi-cli-rs
version=0.7.0
chip=esp32c6
protocol=2
mac=D0:CF:13:E2:90:E8
features=statistics,println,async-print,auto
END-INFO
```

| Key | Use |
|-----|-----|
| `version` | Feature gating (e.g. require `>= 0.7.0` for new WiFi modes) |
| `protocol` | Wire-format version; currently `2` |
| `mac` | Stable device ID for your DB / session map |
| `chip` | UI labels, chip-specific CSI options |
| `features` | Unordered set; `statistics` ⇒ `show-stats` exists |

---

## 3. v0.7.0 changes (esp-csi-rs 0.8.0)

### 3.1 New WiFi operating modes (`set-wifi --mode=`)

| CLI value | `NodeMode` (in `show-config`) | Role |
|-----------|-------------------------------|------|
| `sniffer` | `WifiSniffer` | Passive channel monitor |
| `station` | `WifiStation` | Connect to an existing AP |
| **`wifi-ap`** | **`WifiAccessPoint`** | **SoftAP collector (DHCP + ICMP flood)** |
| `esp-now-central` | `EspNowCentral` | Balanced ESP-NOW central |
| `esp-now-peripheral` | `EspNowPeripheral` | Balanced ESP-NOW peripheral |
| **`esp-now-fast-collector`** | **`EspNowFastCollector`** | **Fast simplex collector** |
| **`esp-now-fast-source`** | **`EspNowFastSource`** | **Fast simplex source** |

### 3.2 New `set-wifi` options

| Argument | Values | Default | Applies to |
|----------|--------|---------|------------|
| `--ap-ssid=<SSID>` | ≤ 32 bytes; quote for spaces | `esp-csi-ap` | `wifi-ap` |
| `--ap-password=<PASSWORD>` | ≤ 32 bytes; empty = open | *(empty)* | `wifi-ap` |
| `--ap-dhcp=<on\|off>` | `on`, `off`, `true`, `false`, `1`, `0`, `yes`, `no` | `on` | `wifi-ap` |
| `--ap-leases=<1-8>` | DHCP lease pool size; > 1 round-robins the ICMP flood across all associated stations | `4` | `wifi-ap` |

Existing options unchanged: `--sta-ssid`, `--sta-password`, `--set-channel`,
`--peer-mac`, `--ht40` (ESP-NOW modes including fast simplex).

### 3.3 New `UserConfig` fields (defaults)

| Field | Default |
|-------|---------|
| `ap_ssid` | `esp-csi-ap` |
| `ap_password` | *(empty — open AP)* |
| `serve_dhcp` | `true` |
| `ap_lease_count` | `4` |

### 3.4 `show-config` additions

The `[WiFi]` section now always includes:

```text
  AP SSID : 'esp-csi-ap'
  AP Pass : open          # or  AP Pass : 'secret'
  AP DHCP : true
  AP Leases: 4
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
SSID: 'esp-csi-ap', Password: (open), DHCP: true, Leases: 4
ESP-NOW Peer MAC: auto
ESP-NOW TX PHY: HT20/legacy
```

### 3.6 Behavior notes for integrators

- **Power saving:** AP, station, and fast ESP-NOW modes disable Wi-Fi power
  saving at collection start (throughput-oriented).
- **`set-rate`:** Applies to all modes except `WifiStation` (including fast
  ESP-NOW). Station derives rate from the associated AP.
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
| `set-wifi …` | `POST /devices/{mac}/wifi` | Mode, channel, STA/AP/ESP-NOW fields |
| `set-traffic --frequency-hz=N` | `POST /devices/{mac}/traffic` | `0` = off |
| `set-collection-mode --mode=collector\|listener` | `POST /devices/{mac}/collection-mode` | |
| `set-protocol --protocol=n` | `POST /devices/{mac}/protocol` | `b\|g\|n\|lr\|a\|ac` |
| `set-rate --rate=mcs7-lgi` | `POST /devices/{mac}/rate` | ESP-NOW / fast modes |
| `set-io-tasks --tx=on --rx=on` | `POST /devices/{mac}/io-tasks` | |
| `set-csi …` | `POST /devices/{mac}/csi` | Chip-specific flags |
| `reset-config` | `POST /devices/{mac}/reset-config` | Restore all defaults |

### 4.2 Immediate commands

| CLI command | Typical web API | Notes |
|-------------|-----------------|-------|
| `set-log-mode --mode=array-list` | `POST /devices/{mac}/log-mode` | Takes effect on next packet |
| `set-csi-delivery --mode=callback` | `POST /devices/{mac}/csi-delivery` | Overridden partially at `start` |
| `info` | `GET /devices/{mac}/info` | Machine-parseable block |
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

### 5.1 `[WiFi]` section (v0.7.0)

```text
[WiFi]
  Mode    : WifiAccessPoint
  Channel : 6
  STA SSID: 'my-router'
  STA Pass: 'secret'
  AP SSID : 'esp-csi-ap'
  AP Pass : open
  AP DHCP : true
  Peer MAC: aa:bb:cc:dd:ee:ff   # or  auto
  TX PHY  : HT20/legacy         # or  HT40 (secondary above|below)
```

**Suggested regex keys:**

| Line prefix | Capture |
|-------------|---------|
| `Mode    : ` | `WifiSniffer`, `WifiStation`, `WifiAccessPoint`, `EspNowCentral`, `EspNowPeripheral`, `EspNowFastCollector`, `EspNowFastSource` |
| `Channel : ` | integer 1–14 |
| `STA SSID: '` | SSID (strip quotes) |
| `AP SSID : '` | SSID (strip quotes) |
| `AP Pass : ` | `open` or quoted password |
| `AP DHCP : ` | `true` / `false` |
| `Peer MAC: ` | MAC or `auto` |
| `Protocol      : ` | *(in `[Collection]` section)* `LR`, `N`, etc. |

### 5.2 Example JSON mapping

```json
{
  "wifi": {
    "mode": "WifiAccessPoint",
    "channel": 6,
    "sta": { "ssid": "", "password": "" },
    "ap": { "ssid": "esp-csi-ap", "password": null, "dhcp": true },
    "peerMac": null,
    "ht40": null
  },
  "collection": {
    "mode": "Collector",
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

### 6.1 SoftAP lab pair (board A = AP collector, board B = station)

**Board A — AP collector**

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

### 6.2 ESP-NOW fast simplex (max CSI packets/sec)

**Collector board**

```text
set-wifi --mode=esp-now-fast-collector --set-channel=6
set-log-mode --mode=array-list
start
```

**Source board**

```text
set-wifi --mode=esp-now-fast-source --set-channel=6
start
```

Optional: pin peers explicitly on both sides:

```text
set-wifi --mode=esp-now-fast-collector --set-channel=6 --peer-mac=aa:bb:cc:dd:ee:ff
```

### 6.3 Balanced ESP-NOW pair (unchanged from v0.6)

```text
# Board 1
set-wifi --mode=esp-now-central --set-channel=6
start

# Board 2
set-wifi --mode=esp-now-peripheral --set-channel=6
start
```

---

## 7. Quoting rules (SSIDs / passwords)

Spaces inside values require single or double quotes on the serial line:

```text
set-wifi --mode=station --sta-ssid='My WiFi' --sta-password="my pass"
set-wifi --mode=wifi-ap --ap-ssid='Lab AP'
```

The firmware preprocessor converts in-quote spaces to `0x1F` internally; your
web server should send properly quoted strings when proxying user input.

Underscores are literal. Values longer than 32 bytes **panic** the device —
validate length server-side.

---

## 8. Suggested REST surface (example)

| Method | Path | Serial equivalent |
|--------|------|-------------------|
| `GET` | `/api/devices` | List sessions keyed by `mac` from last `info` |
| `GET` | `/api/devices/{mac}` | `info` |
| `GET` | `/api/devices/{mac}/config` | `show-config` |
| `PUT` | `/api/devices/{mac}/wifi` | `set-wifi …` (build from JSON body) |
| `PUT` | `/api/devices/{mac}/protocol` | `set-protocol --protocol=…` |
| `PUT` | `/api/devices/{mac}/traffic` | `set-traffic --frequency-hz=…` |
| `POST` | `/api/devices/{mac}/collection/start` | `start` or `start --duration=N` |
| `POST` | `/api/devices/{mac}/collection/stop` | Send `q` on serial |
| `GET` | `/api/devices/{mac}/stats` | `show-stats` |
| `POST` | `/api/devices/{mac}/restart` | `restart` |
| `WS` | `/api/devices/{mac}/csi` | Stream lines emitted during `start` |

Example request body for `PUT /api/devices/{mac}/wifi`:

```json
{
  "mode": "wifi-ap",
  "channel": 6,
  "ap": {
    "ssid": "esp-csi-ap",
    "password": "",
    "dhcp": true
  }
}
```

Maps to:

```text
set-wifi --mode=wifi-ap --set-channel=6 --ap-ssid=esp-csi-ap --ap-password= --ap-dhcp=on
```

---

## 9. Version compatibility

| Firmware | `esp-csi-rs` | New modes | `CLI_PROTOCOL_VERSION` |
|----------|--------------|-----------|--------------------------|
| 0.6.0 | 0.7.x | sniffer, station, esp-now-central/peripheral | 2 |
| **0.7.0** | **0.8.0** | **+ wifi-ap, esp-now-fast-collector/source** | **2** (unchanged) |

Host tooling should:

1. Parse `version` from `info` or the welcome banner.
2. Reject or hide UI for v0.7-only modes when `version < 0.7.0`.
3. Keep using `mac=` for session pinning (`protocol >= 2`).

No breaking changes to the `info` grammar in v0.7.0.

---

## 10. Related files

| File | Purpose |
|------|---------|
| [`SPECS.md`](SPECS.md) | Complete CLI specification (all commands, defaults, edge cases) |
| [`../RELEASE_NOTES.md`](../RELEASE_NOTES.md) | Release changelog |
| [`../README.md`](../README.md) | User-facing usage and build instructions |
