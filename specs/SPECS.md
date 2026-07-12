# `esp-csi-cli-rs` — CLI Specification

A complete, source-accurate reference for the on-device CLI exposed by
`esp-csi-cli-rs` (crate version **0.7.0**). Every command, every argument, every
accepted value, every default, the exact output each command prints, and the
behavioral detail that can be derived from the source.

Sources of truth:
- Command registration / help text: `src/cli/mod.rs` (`ROOT_MENU`)
- Command handlers + printed output: `src/cli/cmds.rs`
- Welcome banner: `src/cli/cli.rs` (`enter_root`)
- Runtime state / defaults: `src/config.rs` (`UserConfig`)
- Input preprocessing, collection task & lifecycle: `src/main.rs`
- Serial backend selection: `src/cli/serial.rs`
- Build features / target gating: `Cargo.toml`

> ⚠️ This document describes firmware behavior over a serial console. The CLI
> runs **on the device**; there is no host-side binary with these commands.
> Interaction is over UART0 or USB-Serial-JTAG via a monitor such as
> `espflash monitor`.
>
> For web-server / host-automation integration (REST mapping, pairing presets,
> v0.7.0 delta), see [`WEBSERVER.md`](WEBSERVER.md).

---

## 1. Operating model

### 1.1 Runtime state

A single `UserConfig` instance lives in the `USER_CONFIG` mutex
(`src/config.rs:134`). It is created with `UserConfig::new()` at boot
(`src/main.rs:256`). Every `set-*` command mutates it in place.

The `csi_collection` Embassy task **snapshots** `USER_CONFIG` (a `clone`) only
on receipt of a `START_SIGNAL` (`src/main.rs:572`) — settings changed *during* a
running collection do **not** apply until the next `start`.

### 1.2 Lifecycle signals

| Signal           | Type                          | Producer                         | Consumer                              | Payload          |
|------------------|-------------------------------|----------------------------------|---------------------------------------|------------------|
| `START_SIGNAL`   | `Signal<Option<u64>>`         | `start` command                  | `csi_collection` task                 | seconds or `None`|
| `STOP_REQUEST`   | `Signal<()>`                  | main loop on `q`/`Q` keypress    | `csi_collection` → `send_stop()`      | `()`             |
| `DONE_SIGNAL`    | `Signal<()>`                  | `csi_collection` at end of run   | main loop (unlocks CLI)               | `()`             |
| `IS_COLLECTING`  | `AtomicBool`                  | `start` (true) / main loop (false)| main loop input gate                  | bool             |

While `IS_COLLECTING == true` the CLI is **locked** (`src/main.rs:374`): the menu
is bypassed and only `q`/`Q` is acted on. All other bytes are read but ignored.

### 1.3 The collection task (what `start` actually triggers)

On a `START_SIGNAL` the task (`src/main.rs:562`):

1. Snapshots `USER_CONFIG`.
2. Maps `node_mode` → an `esp-csi-rs` `Node`:
   - `WifiSniffer` → `Peripheral(WifiSniffer(channel))`
   - `WifiStation` → `Central(WifiStation { ssid, password, WPA2-Personal })`
   - `WifiAccessPoint` → `Central(WifiAccessPoint(WifiApConfig { ap, channel, ht40, dhcp }))`
   - `EspNowCentral` → `Central(EspNow(channel, phy_rate [, peer_mac] [, ht40]))`
   - `EspNowPeripheral` → `Peripheral(EspNow(channel, phy_rate [, peer_mac] [, ht40]))`
   - `EspNowFastCollector` → `Central(EspNowFastCollector(fast_default + channel [, peer_mac] [, ht40]))`
   - `EspNowFastSource` → `Peripheral(EspNowFastSource(fast_default + channel [, peer_mac] [, ht40]))`
3. For `WifiAccessPoint`, `WifiStation`, `EspNowFastCollector`, and
   `EspNowFastSource`, calls `controller.set_power_saving(PowerSaveMode::None)`.
4. Computes traffic frequency: `trigger_freq == 0` → `None` (traffic generator
   off); otherwise `Some(trigger_freq as u16)` (note the `u64 → u16` cast —
   values > 65535 silently truncate).
5. Constructs `CSINode::new(...)`, applies `set_io_tasks(io_tasks)`.
6. Applies `node.set_protocol(user_config.protocol)` (set via `set-protocol`;
   default `LR`) and `node.set_rate(phy_rate)` for all modes **except**
   `WifiStation` (station derives rate from the associated AP).
7. Registers the CSI delivery path for the run:
   - Always calls `set_csi_logging_enabled(false)` first.
   - If `delivery_raw` (`set-csi-delivery --mode=raw`): `set_raw_listen(true)` +
     `set_csi_raw_callback(raw_csi_noop)` — zero-copy, no packet built, no data
     delivered/logged, **no q-key stop peek**.
   - Otherwise: `set_raw_listen(false)` + `set_csi_callback(csi_log_and_check)`.
     This callback runs inline in the WiFi callback for every packet: it (a)
     peeks the JTAG OUT-EP FIFO for `q`/`Q` and signals `STOP_REQUEST`, and (b)
     clones the packet into `log_csi` so CSI lines stream to the host.
8. Runs:
   - `Some(secs)` → `CSINode::run_duration(secs, client)` raced against a stop
     watcher.
   - `None` → `CSINode::run()` raced against the stop watcher (indefinite).
9. On exit, `esp-csi-rs`'s `reset_globals` nulls the callback/gates back to
   `Off`, so the run-time delivery registration is re-applied every `start`.
10. Signals `DONE_SIGNAL` → main loop unlocks the CLI.

> **Important consequence:** because the task re-registers `csi_log_and_check`
> (or the raw no-op) on every `start` and forces `set_csi_logging_enabled(false)`,
> the values you set with `set-csi-delivery` take effect immediately (between
> runs) but are **overridden at the start of each run** by the task's own
> registration. The exception is `--mode=raw`, which is stored in
> `delivery_raw` and consumed by the task at `start`.

### 1.4 Input preprocessing (applies to every command)

Implemented in `src/main.rs:371-545`. The `menu` crate's per-keystroke echo is
disabled; the preprocessor echoes bytes itself so quoting renders correctly.

- **Quoting**: `'…'` and `"…"` both group whitespace. The opening quote style
  must be closed by the same style; the other style appears literally inside.
  Quote characters are echoed but **not** forwarded to the menu tokenizer.
- **Space sentinel**: a space inside quotes is forwarded as **`0x1F`** (US).
  Handlers that take free-form text (`--sta-ssid`, `--sta-password`,
  `--ap-ssid`, `--ap-password`) decode `0x1F → ' '`.
- **Underscores**: passed through literally — no `_ → ' '` substitution.
- **Backspace** (`0x08` / `0x7F`): pops the visible-character shadow buffer;
  quote state is recomputed via `recompute_quote_state`.
- **Newline** (`\r` or `\n`): drops any half-open quote, clears the shadow, and
  on `\r` erases the input line before forwarding to `menu`.
- **Buffer limit**: the input buffer is 256 bytes (`CLI_BUF_LEN`). At capacity
  the terminal bell (`0x07`) rings **once** and further bytes are dropped
  silently (preventing the `menu` crate's per-byte "Buffer overflow!" spam).
  Space frees on backspace / submit.

### 1.5 Serial backend (`src/cli/serial.rs`, `src/main.rs:280`)

| Build                                   | Backend                                                 |
|-----------------------------------------|---------------------------------------------------------|
| `esp32`                                 | UART0 only (RX=GPIO3, TX=GPIO1); no USB-JTAG             |
| `jtag-serial` (non-ESP32)               | Forced `UsbSerialJtag`                                   |
| `uart` (non-ESP32)                      | Forced `Uart` on UART0                                   |
| `auto` (default, non-ESP32)             | Runtime `is_jtag()` probe → `UsbJtag` if USB host present, else `Uart` |

`is_jtag()` reads the USB-Serial-JTAG `SOF` interrupt-raw bit; set ⇒ USB host
present ⇒ JTAG path; clear ⇒ UART0 fallback.

---

## 2. Command reference

All commands are children of the `root` menu (`src/cli/mod.rs:39`). `help` and
`help <command>` are provided automatically by the `menu` crate.

Argument syntax:
- `--flag` — boolean toggle (`Parameter::Named`); presence is the signal.
- `--key=<value>` — value when supplied (`Parameter::NamedValue`).
- `[--key=<value>]` — optional; omission preserves the current state.

---

### 2.1 `set-traffic`

Configure traffic generation frequency.

| Argument             | Type / Values | Default | Meaning                                    |
|----------------------|---------------|---------|--------------------------------------------|
| `--frequency-hz=<N>` | `u64` Hz      | `100`   | `0` disables traffic generation entirely.  |
| `--type=<T>`         | registered, **unused** | — | Reserved no-op (declared in `ROOT_MENU`, ignored by the handler). |

**Output** (always reprints the resulting value):
```

Updated Traffic Configuration:

Traffic Frequency: 100Hz
```

**Behavior / inferences:**
- Stored in `trigger_freq` (`u64`). At `start` it is cast to `u16`; values
  > 65535 truncate silently.
- Invalid (non-numeric) `--frequency-hz` prints `Invalid Frequency` and leaves
  the value unchanged (then still prints the config block).
- If `set-io-tasks --tx=off`, traffic is not generated regardless of frequency.

---

### 2.2 `set-collection-mode`

Set the node role.

| Argument        | Values                    | Default     |
|-----------------|---------------------------|-------------|
| `--mode=<role>` | `collector` \| `listener` | `collector` |

**Output:**
```

Collection Mode: Collector
```

**Behavior:**
- `collector` → `CollectionMode::Collector` (active generation + collection).
- `listener` → `CollectionMode::Listener` (passive receive only).
- Unrecognized value prints `Invalid mode. Use 'collector' or 'listener'.` and
  does **not** mutate state.
- Omitting `--mode` prints
  `Usage: set-collection-mode --mode=<collector|listener>` and does not mutate.

---

### 2.3 `set-log-mode`

Pick the CSI packet output format. Calls
`esp_csi_rs::logging::logging::set_log_mode` directly — takes effect on the
**next** logged packet, no restart needed.

| Argument       | Values                                                   | Default (init) |
|----------------|----------------------------------------------------------|----------------|
| `--mode=<fmt>` | `text` \| `array-list` \| `serialized` \| `esp-csi-tool` | `array-list` (`init_logger` in `src/main.rs:234`) |

**Output:** `\nLog Mode: <Text|ArrayList|Serialized|EspCsiTool>`

**Behavior:**
- `text` → `LogMode::Text` — verbose, human-readable, full metadata.
- `array-list` → `LogMode::ArrayList` — compact CSV-style, one line per packet.
- `serialized` → `LogMode::Serialized` — binary COBS-framed postcard (needs a
  host-side deserializer).
- `esp-csi-tool` → `LogMode::EspCsiTool` — Hernandez 26-column `CSI_DATA,...` CSV
  compatible with the ESP32-CSI-Tool collector.
- Unrecognized value prints
  `Invalid mode. Use 'text', 'array-list', 'serialized', or 'esp-csi-tool'.`;
  missing value prints the `Usage:` hint. Neither mutates.
- **Note:** the README narrative quotes `text` as default, but the logger is
  initialized to `ArrayList` at boot. **Effective initial mode is `ArrayList`.**

---

### 2.4 `set-csi`

CSI feature flags. **Argument set is target-gated** — chips expose different
parameters. Both variants reprint the full CSI configuration after applying.

#### 2.4.1 Classic variant — ESP32, ESP32-C3, ESP32-S3

`#[cfg(not(any(feature = "esp32c5", feature = "esp32c6")))]` (`src/cli/cmds.rs:362`).

| Argument               | Effect (when present)               | Default |
|------------------------|-------------------------------------|---------|
| `--disable-lltf`       | `csi_config.lltf_en = false`        | enabled |
| `--disable-htltf`      | `csi_config.htltf_en = false`       | enabled |
| `--disable-stbc-htltf` | `csi_config.stbc_htltf2_en = false` | enabled |
| `--disable-ltf-merge`  | `csi_config.ltf_merge_en = false`   | enabled |

**Output:**
```

Updated CSI Configuration:

LLTF Enabled: true
HTLTF Enabled: true
STBC HTLTF Enabled: true
LTF Merge Enabled: true
```

These flags are **monotonic** within a session — there is no CLI affordance to
re-enable a disabled flag except `reset-config`.

#### 2.4.2 C5/C6 variant — ESP32-C5, ESP32-C6

`#[cfg(any(feature = "esp32c5", feature = "esp32c6"))]` (`src/cli/cmds.rs:85`).

| Argument                   | Effect (when present)                              | Default |
|----------------------------|----------------------------------------------------|---------|
| `--disable-csi`            | `csi_config.enable = 0`                            | enabled |
| `--disable-csi-legacy`     | `acquire_csi_legacy = 0` (L-LTF / 11g)             | enabled |
| `--disable-csi-ht20`       | `acquire_csi_ht20 = 0`                             | enabled |
| `--disable-csi-ht40`       | `acquire_csi_ht40 = 0`                             | enabled |
| `--val-scale-cfg=<0-3>`    | `val_scale_cfg = N` (`u32`)                        | `2`     |

**Output:**
```

Updated CSI Configuration:

Acquire CSI: 1
Acquire Legacy CSI: 1
Acquire HT20: 1
Acquire HT40: 1
Scale Value: 2
```

**Behavior:**
- `--val-scale-cfg` accepts any `u32`; the `0-3` range
  is documented in help but **not enforced** by the parser.
- A non-numeric value prints `Invalid Max Connections` (a misnamed
  but harmless message) and leaves the field unchanged.
- Per help text: CSI configuration applies to all modes; AP mode uses the same
  `CsiConfig` snapshot as other modes.

---

### 2.5 `set-wifi`

WiFi / radio operating parameters.

| Argument                    | Values                                                                                              | Default   |
|-----------------------------|-----------------------------------------------------------------------------------------------------|-----------|
| `--mode=<m>`                | `station` \| `sniffer` \| `wifi-ap` \| `esp-now-central` \| `esp-now-peripheral` \| `esp-now-fast-collector` \| `esp-now-fast-source` | `sniffer` |
| `--sta-ssid=<SSID>`         | UTF-8, ≤ 32 bytes; quoting allowed                                                                  | empty     |
| `--sta-password=<PASSWORD>` | UTF-8, ≤ 32 bytes; quoting allowed                                                                  | empty     |
| `--ap-ssid=<SSID>`          | UTF-8, ≤ 32 bytes; quoting allowed                                                                  | `esp-csi-ap` |
| `--ap-password=<PASSWORD>`  | UTF-8, ≤ 32 bytes; quoting allowed; empty = open AP                                                 | empty     |
| `--ap-dhcp=<on\|off>`       | `on`/`off`/`true`/`false`/`1`/`0`/`yes`/`no`                                                        | `on`      |
| `--set-channel=<N>`         | `u8`; valid WiFi channels 1–14                                                                      | `1`       |
| `--peer-mac=<MAC>`          | `aa:bb:cc:dd:ee:ff` or `aa-bb-...` (case-insensitive); empty clears                               | auto      |
| `--ht40=<above\|below\|none>` | ESP-NOW forced HT40 TX secondary channel; `none`/`off` clears                                     | none (HT20/legacy) |

**Output** (reprints the resulting WiFi config):
```

Updated WiFi Configuration:

WiFi Mode: WifiSniffer
WiFi Channel: 1
Station WiFi Settings:
SSID: '', Password: ''
Access Point Settings:
SSID: 'esp-csi-ap', Password: (open), DHCP: true
ESP-NOW Peer MAC: auto
ESP-NOW TX PHY: HT20/legacy
```

**Behavior / inferences:**
- Mode → `NodeMode`: `station`→`WifiStation`, `sniffer`→`WifiSniffer`,
  `wifi-ap`→`WifiAccessPoint`, `esp-now-central`→`EspNowCentral`,
  `esp-now-peripheral`→`EspNowPeripheral`, `esp-now-fast-collector`→
  `EspNowFastCollector`, `esp-now-fast-source`→`EspNowFastSource`. Unknown mode
  prints `Invalid WiFi Mode`; field unchanged.
- Channel parsed as `u8`; non-numeric prints `Invalid Max Connections`
  (misnamed). Out-of-range (>14) is accepted by the parser but rejected by the
  radio at `start`. Channel flows to sniffer, AP, and ESP-NOW modes; `WifiStation`
  derives its channel from the associated AP.
- `--sta-ssid` / `--sta-password` / `--ap-ssid` / `--ap-password` are stored in
  `heapless::String<32>`. **A value longer than 32 bytes panics** (`unwrap` on
  `push_str`). Spaces require quoting (arrive as `0x1F`, decoded back here).
  Underscores stay literal.
- `--ap-dhcp`: `on`/`true`/`1`/`yes` → `serve_dhcp = true`; `off`/`false`/`0`/
  `no` → `false`. Invalid value prints `Invalid --ap-dhcp (use on|off)`.
  `wifi-ap` mode only (stored regardless of current mode).
- AP auth: empty `--ap-password` → open AP (`AuthenticationMethod::None`);
  non-empty → WPA2-Personal with the given password.
- `--peer-mac`: a valid MAC switches off automatic magic-prefix pairing in favor
  of explicit per-node source-MAC filtering. An **empty** value resets to `None`
  (auto). A malformed MAC prints `Invalid --peer-mac (use aa:bb:cc:dd:ee:ff)`.
  ESP-NOW modes only (including fast simplex).
- `--ht40`: `above`/`below` force the per-peer ESP-NOW TX PHY to HT40 with that
  secondary channel; `none`/`off` reverts to HT20/legacy. Any other value prints
  `Invalid --ht40 (use above|below|none)`. ESP-NOW modes only (including fast
  simplex). Also passed into `WifiApConfig` for `wifi-ap` when set.

---

### 2.6 `start`

Begin a CSI collection run.

| Argument            | Type  | Default                       |
|---------------------|-------|-------------------------------|
| `--duration=<SECS>` | `u64` | omitted ⇒ run indefinitely    |

**Output:**
- Timed: `Starting CSI collection for <secs>s...`
- Indefinite: `Starting CSI collection indefinitely...`
- Invalid `--duration`: `Invalid duration` and the command aborts (no signal,
  CLI not locked).

**Behavior:**
- Sets `IS_COLLECTING = true` and signals `START_SIGNAL` with `Some(secs)` or
  `None`. See §1.3 for everything the collection task then does.
- Timed run → `CSINode::run_duration(secs)`. Indefinite run → `CSINode::run()`.
  In both, the registered `csi_log_and_check` callback is the sole writer of CSI
  lines (the indefinite path no longer joins a separate print loop).

**Stop conditions:**
- Pressing `q`/`Q` during a run raises `STOP_REQUEST`, forwarded to
  `esp-csi-rs` via `CSINodeClient::send_stop()`, which unwinds `run`/
  `run_duration`. Two parallel reader paths catch the key (§1.3 / `src/main.rs:401`):
  an async `Read::read` arm and a 5 ms `Timer` arm that raw-polls the JTAG OUT-EP
  FIFO — making the stop key deterministic even under ISR starvation. The console
  prints `Stopping...` once, then `Collection complete.` when done.
- A timed run also ends when its duration elapses.
- **`--mode=raw` runs have no q-key peek** — they are duration-bound or
  reset-driven only.
- Both paths fire `DONE_SIGNAL`, unlocking the CLI.

---

### 2.7 `show-config`

Print the current `UserConfig`. **No arguments.**

Sections: `[WiFi]`, `[Collection]`, `[CSI Config]`. The `[CSI Config]` block is
target-gated (HE fields on C5/C6, classic fields elsewhere).

**Output (classic-chip example):**
```

====== Current Configuration ======

[WiFi]
  Mode    : WifiSniffer
  Channel : 1
  STA SSID: ''
  STA Pass: ''
  AP SSID : 'esp-csi-ap'
  AP Pass : open
  AP DHCP : true
  Peer MAC: auto
  TX PHY  : HT20/legacy

[Collection]
  Mode          : Collector
  Traffic Freq  : 100Hz
  PHY Rate      : RateMcs0Lgi
  Protocol      : LR
  IO Tasks      : tx=true, rx=true

[CSI Config]
  LLTF Enabled       : true
  HTLTF Enabled      : true
  STBC HTLTF Enabled : true
  LTF Merge Enabled  : true
  Channel Filter     : <CsiConfig default>
  Manual Scale       : <CsiConfig default>
  Shift Bits         : <CsiConfig default>
  Dump ACK           : <CsiConfig default>

===================================
```

On C5/C6 the `[CSI Config]` block instead prints `Acquire CSI`, `Legacy (11g)`,
`HT20`, `HT40`, `Scale Value`.

The classic branch additionally prints four fields that **cannot be changed via
the CLI**: `channel_filter_en`, `manu_scale`, `shift`, `dump_ack_en`. Restore
them with `reset-config`.

---

### 2.8 `reset-config`

Replace the live `UserConfig` with `UserConfig::new()`. **No arguments.**
Restores **every** field to its compiled-in default (§4).

**Output:** `\nConfiguration Reset to Default Values\n`

---

### 2.9 `set-rate`

Pin the Wi-Fi PHY rate. Applied via `node.set_rate` at `start` for all modes
**except** `WifiStation` (sniffer ignores it in practice; ESP-NOW central /
peripheral and fast simplex consume it).

| Argument     | Values | Default |
|--------------|--------|---------|
| `--rate=<N>` | `1m`/`1m-l`, `2m`, `5m5`/`5m5-l`, `11m`/`11m-l`, `6m`, `9m`, `12m`, `18m`, `24m`, `36m`, `48m`, `54m`, `mcs0-lgi`..`mcs7-lgi`, `mcs0-sgi` | `mcs0-lgi` |

**Output:** `\nPHY Rate: Rate<...>` (Debug form of the `WifiPhyRate` variant).

**Behavior:**
- `1m`/`5m5`/`11m`: only the `-l` (long-preamble) variant exists; the bare names
  alias the same `Rate*mL` enum value.
- Unknown rate prints
  `Invalid rate. Try mcs0-lgi (default), mcs7-lgi, 6m, 24m, 54m, etc.`; no
  mutation.
- Omitting `--rate` prints `Usage: set-rate --rate=<rate>`.

---

### 2.10 `set-protocol`

Set the Wi-Fi PHY protocol applied via `CSINode::set_protocol` at each `start`.

| Argument          | Values                              | Default |
|-------------------|-------------------------------------|---------|
| `--protocol=<p>`  | `b` \| `g` \| `n` \| `lr` \| `a` \| `ac` | `lr` |

**Output:** `\nProtocol: <Debug form>` (e.g. `Protocol: LR`).

**Behavior:**
- Stored in `UserConfig.protocol`; snapshotted at `start`.
- `lr` suits sniffer and ESP-NOW links between ESP devices.
- Use `n` for station mode against a standard AP and
  for AP + STA lab pairs (`wifi-ap` + `station`).
- Unknown protocol prints
  `Invalid protocol. Use one of: b, g, n, lr (default), a, ac.`
- Omitting `--protocol` prints `Usage: set-protocol --protocol=<b|g|n|lr|a|ac>`.

---

### 2.11 `set-io-tasks`

Toggle per-direction TX/RX tasks via `IOTaskConfig`. Applied at `start` via
`node.set_io_tasks(...)`.

| Argument      | Truthy values            | Falsy values              | Default      |
|---------------|--------------------------|---------------------------|--------------|
| `--tx=<bool>` | `on`,`true`,`1`,`yes`    | `off`,`false`,`0`,`no`    | TX enabled   |
| `--rx=<bool>` | (same)                   | (same)                    | RX enabled   |

**Output:** `\nIO Tasks: tx=true, rx=true`

**Behavior:**
- Both arguments optional and independent; omission leaves current value.
- Disabling RX = pure transmitter (skips WiFi-callback CSI path).
- Disabling TX = pure receiver (no traffic generation, regardless of
  `set-traffic`).
- Invalid value prints `Invalid --tx value (use on|off).` (or `--rx`); field
  unchanged. The resulting `tx=…, rx=…` line is still printed.

---

### 2.12 `set-csi-delivery`

Switch the CSI delivery path and the inline log gate. `off`/`callback`/`async`
call `esp_csi_rs::set_csi_delivery_mode` immediately (next packet); `raw` only
sets the `delivery_raw` flag consumed at the next `start`. `--logging` calls
`set_csi_logging_enabled` immediately.

| Argument        | Values                              | Notes |
|-----------------|-------------------------------------|-------|
| `--mode=<m>`    | `off` \| `callback` \| `async` \| `raw` | see below |
| `--logging=<b>` | `on`/`true`/`1`/`yes` ∕ `off`/`false`/`0`/`no` | toggles inline `log_csi` UART/JTAG gate |

Mode meanings:
- `off` → drop user-side dispatch (inline `log_csi` may still run). Output:
  `Delivery mode: Off`.
- `callback` → dispatch synchronously to the registered `set_csi_callback` hook.
  Output: `Delivery mode: Callback`.
- `async` → enqueue for `CSINodeClient::next_csi_packet`. Output:
  `Delivery mode: Async`.
- `raw` → set `delivery_raw = true`; registers the zero-copy `raw_csi_noop`
  fast-path on the next `start` (no packet built, no data delivered or logged,
  no q-key stop). Also skips ESP-NOW control-packet ingest. Output:
  `Delivery mode: Raw (zero-copy fast-path; applies on next start, no CSI data delivered)`.
  `off`/`callback`/`async` all clear `delivery_raw` back to false.
- Unknown mode prints `Invalid mode. Use 'off', 'callback', 'async', or 'raw'.`

`--logging` outputs `Inline CSI logging: ON`/`OFF`; an invalid value prints
`Invalid --logging value (use on|off).`. Both arguments are optional; either or
both may be given in one invocation.

> **Caveat (see §1.3):** the collection task forces `set_csi_logging_enabled(false)`
> and re-registers `csi_log_and_check` (or the raw no-op) on every `start`, so
> `off`/`callback`/`async` and `--logging` are most useful for runtime tweaks
> *between* registrations; only `raw` is durably honored across a `start`.

---

### 2.13 `show-stats`  *(build-gated: `statistics`, default-on)*

Print a one-shot snapshot of runtime CSI / traffic counters. **No arguments.**
When the `statistics` feature is absent the command is **not registered** and
`help` won't list it.

**Output:**
```

====== Runtime Statistics ======
  RX Total Packets : 0
  TX Total Packets : 0
  RX PPS (avg)     : 0
  TX PPS (avg)     : 0
  RX Rate (Hz)     : 0
  TX Rate (Hz)     : 0
  RX Dropped Pkts  : 0
  TX Queued Pkts   : 0
  TX Confirmed Pkts: 0
  TX Failed Pkts   : 0
================================
```

Counters come from `esp_csi_rs::get_*` (`get_total_rx_packets`,
`get_total_tx_packets`, `get_pps_rx`, `get_pps_tx`, `get_rx_rate_hz`,
`get_tx_rate_hz`, `get_dropped_packets_rx`) and the ESP-NOW TX counters
(`get_tx_queued_packets`, `get_tx_confirmed_packets`, `get_tx_failed_packets`).

**Behavior:**
- Counters reset on the start of each new `start` collection.
- TX queued/confirmed/failed are meaningful only in ESP-NOW modes.
- Values remain queryable between runs; not cleared by `reset-config`.

---

### 2.14 `info`

Print a machine-parseable firmware identification block. **No arguments.**

**Output format** (stable within a `protocol` value;
`CLI_PROTOCOL_VERSION = 2`, `src/cli/cmds.rs:26`):
```
ESP-CSI-CLI/<version>
name=esp-csi-cli-rs
version=<version>
chip=<esp32|esp32c3|esp32c5|esp32c6|esp32s3|unknown>
protocol=<u32>
mac=<AA:BB:CC:DD:EE:FF>
features=<comma-separated-list>
END-INFO
```

**Behavior:**
- `name`/`version` from `CARGO_PKG_NAME`/`CARGO_PKG_VERSION` (compile time).
- `chip` resolved from the target feature; `unknown` if no chip feature set.
- `mac` (protocol >= 2) is the factory eFuse base MAC
  (`esp_hal::efuse::base_mac_address`), uppercase colon-separated hex. On
  native-USB boards this is also the USB `iSerialNumber`, so host tooling keys
  per-device tasks off it instead of the enumeration-order `/dev/ttyACM*` path.
  This makes `restart` (§2.14) and the USB re-enumeration it triggers a
  non-event — the per-device task re-binds to the same physical board.
- `features` enumerates the compile-time subset of
  `{statistics, defmt, println, async-print, auto, jtag-serial, uart}` (in that
  emit order, but host code should treat it as an unordered set). Empty if none.
- The first line `ESP-CSI-CLI/<version>` is also the first line of the welcome
  banner on every reset (§6), so a host can identify firmware passively. The
  banner also carries the `mac=` line for the same purpose.

---

### 2.15 `restart`

Reboot the device via `esp_hal::system::software_reset()`. **No arguments.**

**Output:** `\nRestarting...\n`, flushed to the serial transport before the
reset fires; the firmware then reboots and re-emits the welcome banner.

**Behavior / inferences:**
- On native-USB boards (built-in USB-Serial-JTAG) the reset drops and
  re-enumerates the USB device, so the port may return as a different
  `/dev/ttyACM*`. Paired with the `mac=` device key (§2.13, §8), this is a
  non-event: the host re-binds to the same board by serial number.
- Reachable only from the idle CLI — during a collection run the input loop is
  locked to the `q`/`Q` stop key (§1.3), so `restart` cannot be invoked mid-run.
- Diverges (`software_reset` returns `!`); nothing after it runs. On reboot the
  banner (magic line + `mac=`) is re-emitted, which the host greps to confirm
  the board is back.

---

### 2.16 `help`  *(provided by `menu` 0.6)*

| Form             | Effect                                            |
|------------------|---------------------------------------------------|
| `help`           | List all commands with their one-line summaries.  |
| `help <command>` | Print that command's full `help:` block from `ROOT_MENU`. |

---

## 3. Cross-command behavior

### 3.1 Mode-dependent applicability

| Setting            | `WifiSniffer` | `WifiStation` | `WifiAccessPoint` | `EspNowCentral` | `EspNowPeripheral` | `EspNowFastCollector` | `EspNowFastSource` |
|--------------------|:-------------:|:-------------:|:-----------------:|:---------------:|:------------------:|:---------------------:|:------------------:|
| `--set-channel`    | ✅            | ❌ (from AP)  | ✅                | ✅              | ✅                 | ✅                    | ✅                 |
| `--sta-ssid/pwd`   | ❌            | ✅            | ❌                | ❌              | ❌                 | ❌                    | ❌                 |
| `--ap-ssid/pwd/dhcp` | ❌          | ❌            | ✅                | ❌              | ❌                 | ❌                    | ❌                 |
| `--peer-mac`,`--ht40` | ❌         | ❌            | HT40→AP config    | ✅              | ✅                 | ✅                    | ✅                 |
| `set-rate`         | ❌ (no-op)    | ❌            | ✅                | ✅              | ✅                 | ✅                    | ✅                 |
| `set-protocol`     | ✅            | ✅            | ✅                | ✅              | ✅                 | ✅                    | ✅                 |
| Auth method        | n/a           | WPA2-Personal (hardcoded) | None or WPA2 from `--ap-password` | n/a | n/a | n/a | n/a |

### 3.2 Apply timing: snapshot vs. immediate

| Setting category                         | Applied at        | Notes |
|------------------------------------------|-------------------|-------|
| `set-traffic`, `set-collection-mode`, `set-csi`, `set-wifi`, `set-rate`, `set-protocol`, `set-io-tasks` | Next `start` | Snapshotted from `USER_CONFIG` |
| `set-log-mode`                           | Immediate (next packet) | Calls `set_log_mode` directly |
| `set-csi-delivery --mode=off/callback/async`, `--logging` | Immediate (but re-overridden at next `start`) | Calls esp-csi-rs setters directly |
| `set-csi-delivery --mode=raw`            | Next `start`      | Stored in `delivery_raw` |
| Press `q`/`Q`                            | Immediate during a run | Drains via `send_stop()` |

### 3.3 Compile-time gating summary

| Cargo feature                               | Effect on CLI                                              |
|---------------------------------------------|-----------------------------------------------------------|
| `esp32`                                     | UART0 only (RX=GPIO3, TX=GPIO1); no JTAG / `is_jtag`       |
| `esp32c3`/`esp32c5`/`esp32c6`/`esp32s3`     | UART or USB-JTAG; `auto` enables runtime detection         |
| `esp32c5`/`esp32c6`                         | `set-csi` HE/STBC variant; `show-config` HE fields         |
| (other chips)                               | `set-csi` classic LLTF/HTLTF variant                       |
| `statistics` (default)                      | Registers `show-stats`; otherwise omitted entirely         |
| `auto` (default, non-ESP32)                 | `SerialInterface` enum dispatches UART/JTAG at runtime      |
| `jtag-serial`                               | Forces JTAG backend (pulls in `async-print`)               |
| `uart`                                      | Forces UART backend (don't combine with `async-print`)     |
| `println` (default)                         | `println!`-based logging                                   |
| `defmt`                                     | `defmt` binary framing; needs host decoder; **not** with `async-print`/`jtag-serial` |
| `async-print`                               | Non-blocking async logger (auto-enabled by `jtag-serial`)  |

---

## 4. Defaults (`UserConfig::new()`, `src/config.rs:112`)

| Field             | Default                          |
|-------------------|----------------------------------|
| `node_mode`       | `WifiSniffer`                    |
| `collection_mode` | `Collector`                      |
| `trigger_freq`    | `100` Hz                         |
| `sta_ssid`        | empty                            |
| `sta_password`    | empty                            |
| `ap_ssid`         | `esp-csi-ap`                     |
| `ap_password`     | empty (open AP)                  |
| `serve_dhcp`      | `true`                           |
| `csi_config`      | `CsiConfig::default()` (all flags enabled / max detail) |
| `channel`         | `1`                              |
| `phy_rate`        | `WifiPhyRate::RateMcs0Lgi`       |
| `protocol`        | `Protocol::LR`                   |
| `io_tasks`        | TX + RX both enabled             |
| `peer_mac`        | `None` (auto magic-prefix pairing) |
| `ht40_secondary`  | `None` (HT20/legacy)             |
| `delivery_raw`    | `false`                          |

Logger init default (`src/main.rs:234`): `LogMode::ArrayList`.

---

## 5. Error handling & robustness

| Failure                                  | Behavior                                                   |
|------------------------------------------|------------------------------------------------------------|
| Unparsable numeric arg                   | Prints an error string; field unchanged                    |
| Unknown enum value (`--mode`, `--rate`)  | Prints a usage/error hint; field unchanged                 |
| Missing required arg                     | Prints a usage hint; field unchanged                       |
| Unknown command                          | `menu` prints its own "command not found"                  |
| `--sta-ssid`/`--sta-password`/`--ap-ssid`/`--ap-password` > 32 bytes | **Panics** (`heapless::String::push_str().unwrap()`)       |
| `--ap-dhcp` invalid                      | Prints `Invalid --ap-dhcp (use on|off)`; field unchanged   |
| `--peer-mac` malformed                   | Prints `Invalid --peer-mac (...)`; field unchanged         |
| `--ht40` not above/below/none            | Prints `Invalid --ht40 (...)`; field unchanged             |
| `--csi-he-stbc` / `--val-scale-cfg` out of doc range | **Accepted** (range not enforced)              |
| `--set-channel` outside `1..=14`         | Accepted by parser; rejected by radio at `start`           |
| Input line > 256 bytes                   | Bell once, further bytes dropped silently                  |
| Any byte except `q`/`Q` during a run     | CLI locked; byte read but ignored                          |

---

## 6. Quoting & special-character cheat sheet

| Input typed                              | Forwarded to handler            |
|------------------------------------------|---------------------------------|
| `set-wifi --sta-ssid=Foo`                | `Foo`                           |
| `set-wifi --sta-ssid='My WiFi'`          | `My WiFi`                       |
| `set-wifi --sta-ssid="My WiFi"`          | `My WiFi`                       |
| `set-wifi --sta-ssid="O'Brien"`          | `O'Brien` (other quote literal) |
| `set-wifi --sta-ssid='He said "hi"'`     | `He said "hi"`                  |
| `set-wifi --mode=wifi-ap --ap-ssid='Lab AP'` | `Lab AP`                        |

Backspace correctly retracts quote state via `recompute_quote_state`.

---

## 7. Command quick-reference

| Command               | Purpose                              | Apply timing | Build gating  |
|-----------------------|--------------------------------------|--------------|---------------|
| `set-traffic`         | Traffic-gen frequency                | Next `start` | always        |
| `set-collection-mode` | Collector vs Listener                | Next `start` | always        |
| `set-log-mode`        | Output format                        | Immediate    | always        |
| `set-csi`             | CSI feature flags (variant per chip) | Next `start` | classic / HE  |
| `set-wifi`            | Mode / SSID / pass / AP / channel / ESP-NOW peer & HT40 | Next `start` | always |
| `set-protocol`        | Wi-Fi PHY protocol                   | Next `start` | always        |
| `start`               | Begin collection (timed/indefinite)  | —            | always        |
| `show-config`         | Print current config                 | —            | always        |
| `reset-config`        | Restore defaults                     | Next `start` | always        |
| `restart`             | Reboot via software reset            | Immediate    | always        |
| `set-rate`            | PHY rate (all modes except station)  | Next `start` | always        |
| `set-io-tasks`        | Toggle TX / RX tasks                 | Next `start` | always        |
| `set-csi-delivery`    | Delivery mode + inline log gate      | Immediate / `raw` next `start` | always |
| `show-stats`          | Runtime counter snapshot             | —            | `statistics`  |
| `info`                | Firmware identification block        | —            | always        |
| `help [cmd]`          | Help text                            | —            | always (`menu`) |

---

## 8. Firmware identification contract

Two surfaces emit the magic prefix `ESP-CSI-CLI/<version>` so host tooling can
recognize this firmware.

### 8.1 Welcome banner (passive)

`enter_root` (`src/cli/cli.rs:8`) emits the magic line as the **first** line of
the banner on every reset / root-menu re-entry:
```
ESP-CSI-CLI/0.7.0
mac=D0:CF:13:E2:90:E8
******* Welcome to the CSI Collection CLI utility! *******
Available Commands:
    set-wifi                Configure WiFi settings (e.g., mode).
    ...
```
A host can match the first non-bootloader line against
`^ESP-CSI-CLI/\d+\.\d+\.\d+$` after reset — no command roundtrip needed. The
`mac=` line (protocol >= 2) immediately follows, giving the host the device's
stable serial number passively on every reset, so it can re-bind a per-device
task after a `restart`/re-enumeration without a command roundtrip.

### 8.2 `info` command (active)

For on-demand identification, invoke `info` (§2.13) — same magic prefix plus a
`key=value` body terminated by `END-INFO`.

### 8.3 Versioning rules

- `version` (after `ESP-CSI-CLI/`) is cosmetic; bumps with releases.
- `protocol` (`CLI_PROTOCOL_VERSION`) is the wire-format version; host tooling
  should refuse `protocol` values it does not understand. The `info` grammar
  (line order, keys, sentinel) is stable within a `protocol` value; adding
  lines/keys requires a `protocol` bump.
- `features` is informational; presence of `statistics` tells the host whether
  `show-stats` exists. Treat the list as an unordered set.
- `mac` (added in `protocol = 2`) is the stable device key. Host tooling pins
  per-device tasks to it instead of the `/dev/ttyACM*` path, so a `restart` /
  USB re-enumeration re-binds to the same physical board.
