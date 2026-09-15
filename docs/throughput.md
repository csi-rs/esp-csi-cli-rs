# Console throughput

What limits the CSI rate you observe, and which of it is the console
rather than the radio.

## Console throughput

**The console, not the radio, is usually what limits your sample rate.** A collector captures at the
offered traffic rate; whether you *see* those samples depends on how many bytes each one costs on the
serial link.

At the default 115200 baud, an `array-list` line for a 384-sample payload is ~2.2 kB — about 190 ms
on the wire. That write is **blocking and happens inside the Wi-Fi RX callback**, so a line that
takes longer to send costs you the CSI reports that arrive while it is sending. Bytes per sample is
therefore the number that decides your rate.

Measured on two ESP32-WROOM boards over a CH340 bridge, 400 Hz offered, station RX-only:

| log mode / payload | @115200 | @921600 |
|---|---|---|
| `array-list`, all LTF (384 samples) | 8.9 CSI/s | 60.4 CSI/s |
| `array-list`, L-LTF only (128 samples) | 22.7 CSI/s | 115.2 CSI/s |
| `serialized` (binary), all LTF | 27.5 CSI/s | 120.0 CSI/s |

At 115200 the `array-list` row is pushing ~10.8 kB/s of an 11.5 kB/s line — the wire is saturated, so
that figure is the ceiling for that format at that baud. At 921600 `array-list` runs at ~86% of line
rate, and what limits the other two rows there is how many frames the radio captured, not the
console.

Expect run-to-run variance: on a shared 2.4 GHz channel the number of frames a collector actually
captures moved by more than 2x between sessions on the same bench, with the boards untouched. Compare
against `show-stats`' own `RX Total Packets` rather than against a number from a previous run.

Three levers, cheapest first:

1. **`set-csi --htltf=off --stbc-htltf=off`** — acquire only the L-LTF field. ~2.5x at 115200, no
   rebuild, and every sample you get is still a complete field. This is the right way to shorten a
   CSI line: the radio stops *capturing* what you do not want, so it also does less work per frame
   and captures more frames (measured 439 vs 205 captured over the same 15 s). Use it when the
   64-subcarrier legacy estimate is enough for your application.
2. **`set-log-mode --mode=serialized`** — binary COBS/postcard instead of decimal text. ~3x at
   115200 while keeping the full payload. Requires a matching deserializer on the host.
3. **Raise the console baud** — see below. The only lever that removes the ceiling rather than
   working under it: ~7x for `array-list` at 921600.

> 📝 There is deliberately **no option to truncate a CSI payload** to a sample count. Cutting a line
> short discards subcarriers the radio already spent airtime capturing, and an arbitrary cap lands
> mid-LTF-field, which is not a meaningful measurement. Lever 1 does the same thing properly and
> measured *faster* than truncating to the same 128 bytes (22.7 vs 21.0 CSI/s, and 439 vs 446 frames
> captured), because the saving happens before the capture rather than after it.

A fourth, if third-party traffic is part of the problem: `set-csi-filter` drops frames before they
are ever formatted, so everything it rejects is console bandwidth handed back to the traffic you
configured.

### `defmt` does not raise the CSI rate

A `defmt` build is **not** a throughput lever, despite being the more efficient logger in general.
Measured on the same pair of boards at 115200, station RX-only:

| config | `println` | `defmt` |
|---|---|---|
| `array-list`, 384 samples | 9.26 CSI/s (1185 B/sample) | 9.53 CSI/s (1176 B/sample) |
| `serialized` | 27.80 CSI/s (399 B/sample) | 27.93 CSI/s (389 B/sample) |

Within a few percent either way — run-to-run noise — and every row sits at 94-97% of line rate in
both builds. The reason is that `defmt` earns its keep by interning *format strings* at compile time,
and a CSI line has none to intern: the line is formatted first and then handed to `defmt` as a single
runtime `{=str}` argument, so the bytes on the wire are the same ASCII either way. In `serialized`
mode it is mildly counterproductive — the COBS frame is wrapped in a `defmt` frame, so you pay both
framings.

Choose `defmt` for what it is actually good at — compact structured logs and host-side decoding
against the ELF — and reach for the levers above for CSI throughput.

Verify with `show-stats`: `RX Total Packets` counts what the radio captured and `RX Dropped Pkts`
counts what was discarded, so the gap between that and the rows your host recorded is the console
loss. Counters are zeroed at the start of each `start` and **survive** the end of the run, so read
them after stopping.

### Console baud is a build-time setting

The baud rate is fixed when you build, via the `ESP_CSI_CLI_UART_BAUD` environment variable
(default `115200`), and there is deliberately **no runtime command** to change it: a `set-uart`
command would have to change the rate of the very console carrying the command, so the reply is
either lost or sent at the old rate while the host has already switched, and surviving a reset would
need a persisted setting or a renegotiation handshake. Instead the rate is immutable per image and
*declared* — `info` reports it as `baud=`, and the release `manifest.json` carries it per asset for
tooling that must pick a rate before the first byte.

The variable composes with every existing alias, so there is no parallel set of aliases to keep in
step:

```bash
ESP_CSI_CLI_UART_BAUD=921600 cargo esp32-build
ESP_CSI_CLI_UART_BAUD=460800 cargo esp32c6-defmt
```

Or commit the choice for a checkout by adding it to `.cargo/config.toml`, which is the better home
for it — the rate then travels with the repo instead of living in whoever's shell history:

```toml
[env]
ESP_CSI_CLI_UART_BAUD = { value = "921600", force = true }
```

`force = true` makes the committed value win over a stale `ESP_CSI_CLI_UART_BAUD` already exported in
a shell. Changing either the variable or that line rebuilds — the build script declares
`rerun-if-env-changed`, so an image can never silently keep a previously compiled rate.

> 🛑 Open your serial port at the matching rate — `espflash monitor --baud 921600`, or
> `serial.Serial(port, 921600)`. The build prints a warning naming the rate whenever it is not the
> default.

> 📝 The **ROM bootloader banner is always emitted at 115200**, whatever you build at. The first few
> lines after a reset are therefore unreadable at a higher rate. This is normal; the firmware's own
> banner (`ESP-CSI-CLI/<version>`) arrives at the configured rate.
