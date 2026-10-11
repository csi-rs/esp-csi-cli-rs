# esp-csi-cli-rs — Web Server Integration Guide

This document describes how to drive `esp-csi-cli-rs` **v0.10.0** (`protocol=4`) from a host
web server or automation layer over a serial transport (UART or USB-Serial-JTAG).
It focuses on wire formats, the command surface, and multi-device pairing flows.
The vocabulary (mode, session role, sensing role, reporting — IEEE 802.11bf terms) follows the
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

Send `info\r`. Response (stable for `protocol=4`; 4 = the IEEE 802.11bf command vocabulary, with the esp-csi-rs 0.12 `wire` serialized frames of protocol 3 unchanged):

```text
ESP-CSI-CLI/0.10.0
name=esp-csi-cli-rs
version=0.10.0
chip=esp32c6
protocol=4
mac=D0:CF:13:E2:90:E8
log=text
transport=auto
baud=115200
features=statistics,println,auto
END-INFO
```

| Key | Use |
|-----|-----|
| `version` | Feature gating (e.g. require `>= 0.10.0` for `--reporting`) |
| `protocol` | Command / wire-format version; currently `4`. A host built for `4` must refuse `3` (it would send commands that firmware rejects) |
| `mac` | Stable device ID for your DB / session map |
| `chip` | UI labels, chip-specific CSI options |
| `log` | `text` or `defmt`: which decoder the log frames need |
| `transport` | `auto`, `jtag` or `uart`: the console the build writes to |
| `baud` | UART rate the build was compiled with |
| `features` | Unordered set; `statistics` ⇒ `show-stats` exists |

Parsers should ignore unknown keys: `info` gains lines without a `protocol` bump.

---

## 3. Mode and option surface

### 3.1 Modes (`set-wifi --mode=`)

Each `--mode` token selects a mode (how the node reaches the channel) and, where
the link has two ends, the session role (initiator or responder). The sensing
role follows from the mode. Reporting is set separately with `--reporting`, and
only where the mode admits a choice. `show-config` and the `set-wifi`
confirmation print the token back on a `Mode:` line.

| CLI token | Mode | Session role | Sensing role | Reporting |
|-----------|------|--------------|--------------|-----------|
| `sniffer` | Wi-Fi sniffer | none (observer) | receiver | `--reporting` (not `none`) |
| `station` | Wi-Fi station | initiator while periodicity > 0, else none | transmitter-receiver / receiver | `--reporting` |
| `wifi-ap` | Wi-Fi access point (DHCP + ICMP flood) | initiator | receiver | `--reporting` |
| `ht20-emitter` | Emitter (20 MHz HT, TX only) | none | transmitter | none (fixed) |
| `ht40-emitter` | Emitter (40 MHz HT, TX only) | none | transmitter | none (fixed) |
| `esp-now-initiator` | ESP-NOW (sources the traffic) | initiator | transmitter-receiver | `--reporting` |
| `esp-now-responder` | ESP-NOW (replies) | responder | transmitter-receiver | `--reporting` |
| `esp-now-simplex-source` | ESP-NOW simplex (unicast flood at forced PHY) | initiator | transmitter | none (fixed) |
| `esp-now-simplex-peer` | ESP-NOW simplex (sparse beacon, then RX-only) | responder | receiver | report (fixed) |

Both members of an ESP-NOW pair, simplex included, must share `--set-channel`.
Pairing is automatic unless `--peer-mac` is set on **both** nodes. An emitter
needs no pair at all — any number of `sniffer`s on its channel will hear it.

There are no aliases: the pre-0.10 tokens (`esp-now-central`,
`esp-now-peripheral`, `esp-now-fast-source`, `esp-now-fast-collector`) are
rejected with `Invalid WiFi Mode`.

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
| `--reporting=<report\|none\|threshold\|decimate>` | Reporting; `none` captures but reports nothing | `report` | `station`, `wifi-ap`, `esp-now-initiator`, `esp-now-responder`, `sniffer` (not `none`: refused with an error); ignored elsewhere (the mode fixes it) |
| `--report-to=<host\|local>` | Where `report` sends measurements; `local` = on-device consumers only | `host` | as `--reporting` |

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
| `reporting` | `Report { to: Host }` |
| `periodicity_hz` | `100` |
| `inject_period_us` | `20000` |

### 3.4 `show-config` node block

`show-config` opens with a `[Node]` block, read back from the mode the next run
is built with:

```text
[Node]
  Mode: esp-now-responder
  Session role: responder
  Sensing role: transmitter-receiver (fixed by mode)
  Reporting: report (host)
  Periodicity: 100 Hz
```

`Session role` is `initiator` | `responder` | `none (observer)`; `Sensing role`
is `transmitter` | `receiver` | `transmitter-receiver`; `Reporting` is
`report (host)` | `report (local)` | `none` | `threshold (level=N, hold=Nms)` |
`decimate N`. A value the mode does not let you choose carries `(fixed by mode)`.

The `[WiFi]` section includes:

```text
  AP SSID : 'esp-csi-ap'
  AP Pass : open          # or  AP Pass : 'secret'
  AP DHCP : true
  AP Leases: 4
  AP Burst : false
```

### 3.5 `set-wifi` confirmation output

After `set-wifi`, the device prints the node attributes and an **Access Point Settings** block:

```text
Updated WiFi Configuration:

Mode: wifi-ap
Session role: initiator (fixed by mode)
Sensing role: receiver (fixed by mode)
Reporting: report (host)
WiFi Channel: 6
Station WiFi Settings:
SSID: '', Password: ''
Access Point Settings:
SSID: 'esp-csi-ap', Password: (open), DHCP: true, Leases: 4, Burst: false
Peer MAC: unset (emitter broadcasts / ESP-NOW auto-pairs)
Secondary Channel: HT20/legacy
Emitter Period: 20000us
```

### 3.6 Behavior notes for integrators

- **Reporting vs. output gate:** the node's reporting is
  `set-wifi --reporting=report|none|threshold|decimate` (or
  `set-reporting --policy=...` with its parameters), read only by `station`,
  `wifi-ap`, `esp-now-initiator`, `esp-now-responder` and the sniffer (the
  ESP-NOW modes also announce it to the peer, so a responder whose initiator
  reports none promotes itself to `report`). `set-csi-output --enabled=<true|false>` is the
  runtime delivery gate: with delivery off the radio still captures (RX path and
  timing unchanged) but nothing is decoded or logged.
- **Power saving:** AP and station modes disable Wi-Fi power saving at collection
  start (throughput-oriented).
- **`set-rate`:** Reporting only, except on the ESP-NOW pair:
  `esp-now-initiator` / `esp-now-responder` apply it as the per-peer TX PHY.
  Station, sniffer and wifi-ap take their rate from the surrounding radio
  configuration, an emitter's rate follows its forced TX PHY, and the simplex
  source uses its profile's fixed rate.
- **`set-protocol`:** User-configurable (`lr` default). For AP + STA lab pairs
  use `n` on **both** boards.
- **`set-measurement --periodicity-hz`:** Default 100 Hz. Library AP/STA examples use 4000 Hz;
  expose this as a “high throughput” preset in your UI.

---

## 4. Command reference (web-server mapping)

Commands are sent as a single line (no shell). Example: `set-wifi --mode=wifi-ap --set-channel=6\r`

### 4.1 Configuration commands (apply on next `start`)

| CLI command | Typical web API | Notes |
|-------------|-----------------|-------|
| `set-wifi …` | `POST /devices/{mac}/config/wifi` | Mode, channel, STA/AP/emitter/ESP-NOW fields; JSON `reporting: "report"\|"none"` → `--reporting=` (csi-webserver-core 0.4) |
| `set-measurement --periodicity-hz=N [--unsolicited=on\|off]` | `POST /devices/{mac}/config/measurement` `{"periodicity_hz": N}` | `0` = off (csi-webserver-core 0.4) |
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

### 5.1 `[Node]` and `[WiFi]` sections

```text
[Node]
  Mode: wifi-ap
  Session role: initiator (fixed by mode)
  Sensing role: receiver (fixed by mode)
  Reporting: report (host)
  Periodicity: 4000 Hz

[WiFi]
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
| `Mode: ` | a `set-wifi --mode=` token (§3.1) |
| `Session role: ` | `initiator`, `responder` or `none (observer)`, optionally followed by `(fixed by mode)` |
| `Sensing role: ` | `transmitter`, `receiver` or `transmitter-receiver`, followed by `(fixed by mode)` |
| `Reporting: ` | `report (host)`, `report (local)`, `none`, `threshold (...)` or `decimate N`, optionally followed by `(fixed by mode)` |
| `Periodicity: ` | integer, then ` Hz` |
| `Channel : ` | integer 1–14 |
| `STA SSID: '` | SSID (strip quotes) |
| `AP SSID : '` | SSID (strip quotes) |
| `AP Pass : ` | `open` or quoted password |
| `AP DHCP : ` | `true` / `false` |
| `Dst MAC : ` | MAC or `broadcast` |
| `Report to     : ` | *(in `[Measurement]` section)* `host` or `local` |
| `Protocol      : ` | *(in `[Measurement]` section)* `LR`, `N`, etc. |

### 5.2 Example JSON mapping

```json
{
  "wifi": {
    "mode": "wifi-ap",
    "reporting": "report",
    "channel": 6,
    "sta": { "ssid": "", "password": "" },
    "ap": { "ssid": "esp-csi-ap", "password": null, "dhcp": true },
    "dstMac": null,
    "apSecondary": null
  },
  "measurement": {
    "csiOutput": true,
    "periodicityHz": 100,
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

**Board A — access point (the initiator; reports)**

```text
reset-config
set-wifi --mode=wifi-ap --set-channel=6 --ap-ssid=esp-csi-ap
set-protocol --protocol=n
set-measurement --periodicity-hz=4000
set-log-mode --mode=array-list
show-config
start
```

**Board B — station (match AP SSID and channel)**

```text
reset-config
set-wifi --mode=station --sta-ssid=esp-csi-ap --set-channel=6
set-protocol --protocol=n
set-measurement --periodicity-hz=4000
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
`periodicity-hz / N` packets per second.

### 6.2 Emitter + sniffer

**Sniffer board** (observer, receiver, reports) — lock the emitter's channel and measure every frame overheard.

```
set-wifi --mode=sniffer --set-channel=6
set-measurement --periodicity-hz=0
set-log-mode --mode=array-list
start
```

**Emitter board** (transmitter, reports none) — 20 MHz; use `ht40-emitter` for 40 MHz bonded.

```
set-wifi --mode=ht20-emitter --set-channel=6 --inject-period-us=20000
start
```

An emitter never associates and its frames carry no payload meaning, so a single
emitter sounds every sniffer in range at once. Unicasting with `--peer-mac` to
one sniffer tends to raise that sniffer's CSI callback rate.

### 6.3 ESP-NOW initiator + responder

Connectionless: no AP, no DHCP, no association. Both ends capture CSI, and each
reports unless set to `--reporting=none`.

**Initiator board** — sources the traffic.

```
set-wifi --mode=esp-now-initiator --set-channel=6
set-rate --rate=mcs0-lgi
set-log-mode --mode=array-list
start
```

**Responder board** — replies on the same channel.

```
set-wifi --mode=esp-now-responder --set-channel=6
set-rate --rate=mcs0-lgi
set-log-mode --mode=array-list
start
```

With more than two boards on one channel, set `--peer-mac` on both to pin the
pair explicitly rather than relying on magic-prefix discovery.

### 6.4 ESP-NOW simplex pair (highest CSI rate)

Asymmetric on purpose: the peer (responder, a receiver that reports) beacons
sparsely until it hears a source, then stops transmitting and goes RX-only, so
all airtime belongs to the source (initiator, a transmitter that reports none).
Start the peer first.

```
# Peer board (responder)
set-wifi --mode=esp-now-simplex-peer --set-channel=6
set-log-mode --mode=serialized
start

# Source board (initiator)
set-wifi --mode=esp-now-simplex-source --set-channel=6
start
```

To pin the pair, set `--peer-mac` on both boards. `set-rate` does not apply to this
pair — the simplex profile fixes its own PHY.

