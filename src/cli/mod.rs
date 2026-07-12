mod cli;
mod cmds;
mod serial;

use cli::enter_root;
use menu::{Item, ItemType, Menu, Parameter};

#[cfg(feature = "statistics")]
use crate::cli::cmds::show_stats;
use crate::cli::cmds::{
    cli_info, reset_config, restart_cmd, set_collection_mode, set_csi, set_csi_delivery_cmd,
    set_io_tasks_cmd, set_log_mode, set_phy_rate, set_protocol_cmd, set_traffic, set_wifi,
    show_config, start_csi_collect,
};
pub use crate::cli::serial::SerialInterface;
// `is_jtag` is only compiled under `auto` (see serial.rs); match that gating
// here so forced `jtag-serial`/`uart` builds don't try to re-export a fn that
// doesn't exist.
#[cfg(all(
    feature = "auto",
    any(
        feature = "esp32c3",
        feature = "esp32c5",
        feature = "esp32c6",
        feature = "esp32s3"
    )
))]
pub use crate::cli::serial::is_jtag;

/// Placeholder context passed through the `menu` crate to every command callback.
///
/// Currently unused but required by the [`menu::Runner`] API. It can be extended
/// in the future to carry session state between commands.
#[derive(Default)]
pub struct Context {
    _inner: u32,
}

// CLI Root Menu Struct Initialization
pub const ROOT_MENU: Menu<SerialInterface, Context> = Menu {
    label: "root",
    items: &[
        &Item {
            item_type: ItemType::Callback {
                function: set_traffic,
                parameters: &[
                    Parameter::NamedValue {
                        parameter_name: "type",
                        argument_name: "type",
                        help: Some("Traffic Type"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "frequency-hz",
                        argument_name: "frequency-hz",
                        help: Some("Traffic Generation Frequency"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "unsolicited",
                        argument_name: "unsolicited",
                        help: Some("Flood unsolicited echo replies instead of requests (on|off)"),
                    },
                ],
            },
            command: "set-traffic",
            help: Some(
                "set-traffic - Configure traffic-related parameters.

Usage:
  set-traffic [OPTIONS]

Options:
  --frequency-hz=<NUMBER>      Specify the traffic frequency in Hz (default: 100).
  --unsolicited=<on|off>       Flood unsolicited ICMP echo REPLIES instead of
                               echo requests (default: off). The peer ignores
                               unsolicited replies, so traffic is strictly
                               one-directional: no reply contention, stable
                               offered rate — but this node gets no CSI back.
                               Use on a flooding AP whose paired station is
                               the collector; keep off when this node needs
                               CSI from the peer's replies (e.g. station
                               flooding a router).

Examples:
  set-traffic --frequency-hz=10
  set-traffic --frequency-hz=1000 --unsolicited=on

Description:
  This command allows you to configure traffic parameters for the CSI collection process.
  You can enable traffic generation and specify the interval
  between generated packets. Setting a value of zero disables traffic
  generation (WiFi AP/station modes: the ICMP flood TX task is not started;
  a receive-only collector should set this to 0 so it does not contend for
  airtime with the AP's downlink flood).",
            ),
        },
        &Item {
            item_type: ItemType::Callback {
                function: set_collection_mode,
                parameters: &[
                    Parameter::NamedValue {
                        parameter_name: "mode",
                        argument_name: "mode",
                        help: Some("Collection mode: 'collector' or 'listener'"),
                    },
                ],
            },
            command: "set-collection-mode",
            help: Some("set-collection-mode - Set the CSI node collection mode.

Usage:
  set-collection-mode --mode=<collector|listener>

Options:
  --mode=collector    Act as the node that generates and collects CSI data (default).
  --mode=listener     Act as a passive listener that only receives CSI data.

Examples:
  set-collection-mode --mode=collector
  set-collection-mode --mode=listener"),
        },
        &Item {
            item_type: ItemType::Callback {
                function: set_log_mode,
                parameters: &[
                    Parameter::NamedValue {
                        parameter_name: "mode",
                        argument_name: "mode",
                        help: Some("Log mode: 'text', 'array-list', 'serialized', or 'esp-csi-tool'"),
                    },
                ],
            },
            command: "set-log-mode",
            help: Some("set-log-mode - Set the CSI output logging format.

Usage:
  set-log-mode --mode=<text|array-list|serialized|esp-csi-tool>

Options:
  --mode=text           Human-readable verbose output with metadata (default).
  --mode=array-list     Compact CSV-style array output, one line per packet.
  --mode=serialized     Binary COBS-framed postcard format for host-side parsing.
  --mode=esp-csi-tool   Hernandez-style 26-column CSV (`CSI_DATA,...` lines) for
                        compatibility with the ESP32-CSI-Tool collector.

Examples:
  set-log-mode --mode=text
  set-log-mode --mode=array-list
  set-log-mode --mode=esp-csi-tool"),
        },
        #[cfg(not(any(feature = "esp32c5", feature = "esp32c6")))]
        &Item {
            item_type: ItemType::Callback {
                function: set_csi,
                parameters: &[
                    Parameter::NamedValue {
                        parameter_name: "lltf",
                        argument_name: "onoff",
                        help: Some("LLTF CSI: on|off"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "htltf",
                        argument_name: "onoff",
                        help: Some("HTLTF CSI: on|off"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "stbc-htltf",
                        argument_name: "onoff",
                        help: Some("STBC HTLTF CSI: on|off"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "ltf-merge",
                        argument_name: "onoff",
                        help: Some("LTF Merge: on|off"),
                    },
                ],
            },
            command: "set-csi",
            help: Some("set-csi - Configure CSI feature flags.

Usage:
    set-csi [OPTIONS]

    Each flag is an on|off toggle, so a feature can be re-enabled after being
    turned off (no reset-config needed). Accepted values: on|off, true|false,
    1|0, enable|disable, yes|no.

    Options:
    --lltf=<on|off>              LLTF CSI configuration (default: on).
    --htltf=<on|off>             HTLTF CSI configuration (default: on).
    --stbc-htltf=<on|off>        STBC HTLTF CSI configuration (default: on).
    --ltf-merge=<on|off>         LTF Merge CSI configuration (default: on).

Examples:
    set-csi --lltf=off --ltf-merge=off
    set-csi --htltf=on

Description:
This command allows you to enable or disable specific Channel State Information (CSI) features.
By default, all CSI features are enabled. Use the options to toggle specific
configurations on or off.

Note:
CSI Configuration is ignored when running in Access Point Mode."),
        },
        #[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
        &Item {
            item_type: ItemType::Callback {
                function: set_csi,
                parameters: &[
                    Parameter::NamedValue {
                        parameter_name: "csi",
                        argument_name: "onoff",
                        help: Some("Acquisition of CSI: on|off"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "csi-legacy",
                        argument_name: "onoff",
                        help: Some("L-LTF on 11g PPDU: on|off"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "csi-ht20",
                        argument_name: "onoff",
                        help: Some("HT-LTF on HT20 PPDU: on|off"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "csi-ht40",
                        argument_name: "onoff",
                        help: Some("HT-LTF on HT40 PPDU: on|off"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "val-scale-cfg",
                        argument_name: "valscalecfg",
                        help: Some("Value 0-3"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "preset",
                        argument_name: "preset",
                        help: Some("CSI preset: default"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "dump-ack",
                        argument_name: "onoff",
                        help: Some("Dump 802.11 ACK frames: on|off"),
                    },
                    #[cfg(feature = "esp32c5")]
                    Parameter::NamedValue {
                        parameter_name: "csi-force-lltf",
                        argument_name: "onoff",
                        help: Some("Force L-LTF acquisition (C5 only): on|off"),
                    },
                    #[cfg(feature = "esp32c5")]
                    Parameter::NamedValue {
                        parameter_name: "csi-vht",
                        argument_name: "onoff",
                        help: Some("VHT-LTF on VHT20 PPDU (C5 only): on|off"),
                    },
                ],
            },
            command: "set-csi",
            help: Some("set-csi - Configure CSI feature flags.

Usage:
    set-csi [OPTIONS]

    Each acquisition flag is an on|off toggle, so a feature can be re-enabled
    after being turned off (no reset-config needed). Accepted values: on|off,
    true|false, 1|0, enable|disable, yes|no.

    Options:
    --csi=<on|off>              Acquisition of CSI, master switch (default: on)
    --csi-legacy=<on|off>       L-LTF when receiving a 11g PPDU (default: on)
    --csi-ht20=<on|off>         HT-LTF when receiving an HT20 PPDU (default: on)
    --csi-ht40=<on|off>         HT-LTF when receiving an HT40 PPDU (default: on)
    --val-scale-cfg             Value 0-3 (default: 2)
    --preset=<default>          Apply a CSI acquisition preset
    --dump-ack=<on|off>         Dump 802.11 ACK frames (default: on)
    --csi-force-lltf=<on|off>   Force L-LTF acquisition (ESP32-C5 only)
    --csi-vht=<on|off>          VHT-LTF on VHT20 PPDUs (ESP32-C5 only)

Examples:
    set-csi --csi-legacy=off --preset=default
    set-csi --csi-ht40=on --csi-ht20=off
    set-csi --csi=off

Description:
This command allows you to enable or disable specific Channel State Information (CSI) features.
By default, all CSI features are enabled. Use the options to toggle specific
configurations on or off."),
        },
        &Item {
            item_type: ItemType::Callback {
                function: set_wifi,
                parameters: &[
                    Parameter::NamedValue {
                        parameter_name: "mode",
                        argument_name: "wifimode",
                        help: Some("Specify operation mode"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "sta-ssid",
                        argument_name: "stassid",
                        help: Some("The SSID for the station"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "sta-password",
                        argument_name: "stapassword",
                        help: Some("The password for the station"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "ap-ssid",
                        argument_name: "apssid",
                        help: Some("The SSID for the softAP (wifi-ap mode)"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "ap-password",
                        argument_name: "appassword",
                        help: Some("The password for the softAP (empty = open)"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "ap-dhcp",
                        argument_name: "apdhcp",
                        help: Some("Enable/disable built-in DHCP in wifi-ap mode (on|off)"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "ap-leases",
                        argument_name: "apleases",
                        help: Some("DHCP lease pool size in wifi-ap mode (1-8)"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "ap-burst",
                        argument_name: "apburst",
                        help: Some("Synchronized burst flood in wifi-ap mode (on|off)"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "set-channel",
                        argument_name: "wifichannel",
                        help: Some("Specify the channel"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "peer-mac",
                        argument_name: "peermac",
                        help: Some("ESP-NOW explicit peer MAC (aa:bb:cc:dd:ee:ff); empty clears"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "ht40",
                        argument_name: "ht40",
                        help: Some("ESP-NOW forced HT40 TX PHY: above|below|none"),
                    },
                ],
            },
            command: "set-wifi",
            help: Some("set-wifi - Configure WiFi settings.

Usage:
  set-wifi [OPTIONS]

NOTE: For SSIDs/passwords containing spaces, wrap the value in single or double
quotes, e.g. --sta-ssid='My WiFi' --sta-password=\"my pass\". Both quote styles
are accepted. Underscores are passed through as literal `_`.

Options:
  --mode=<station|sniffer|wifi-ap|esp-now-central|esp-now-peripheral|esp-now-fast-collector|esp-now-fast-source>
                                                                Specify WiFi operation mode (default: sniffer).
  --sta-ssid=<SSID>                                             Set the SSID for the station (default: empty).
  --sta-password=<PASSWORD>                                     Set the password for the station (default: empty).
  --ap-ssid=<SSID>                                              Set the SSID for wifi-ap mode (default: esp-csi-ap).
  --ap-password=<PASSWORD>                                      Set the softAP password (default: empty = open).
  --ap-dhcp=<on|off>                                            Enable/disable built-in DHCP in wifi-ap mode (default: on).
  --ap-leases=<1-8>                                             DHCP lease pool size in wifi-ap mode (default: 4). With
                                                                more than one lease the ICMP flood round-robins across
                                                                all associated stations; 1 = legacy single-target flood.
  --ap-burst=<on|off>                                           Synchronized burst flood in wifi-ap mode (default: off).
                                                                Every tick sends one frame back-to-back to every station
                                                                for time-aligned multi-receiver CSI; total airtime =
                                                                frequency-hz x leases (off = round-robin, rate shared).
  --set-channel=<NUMBER>                                        Set the channel (default: 149 on C5, 1 elsewhere).
  --peer-mac=<aa:bb:cc:dd:ee:ff>                                ESP-NOW: explicit peer MAC. Switches off automatic
                                                                magic-prefix pairing for per-node source-MAC filtering.
                                                                Pass an empty value to clear (back to auto pairing).
  --ht40=<above|below|none>                                     ESP-NOW: force the per-peer TX PHY to HT40 with the
                                                                given secondary channel (default: none = HT20/legacy).

Examples:
  set-wifi --mode=sniffer
  set-wifi --mode=station --sta-ssid='My WiFi' --sta-password='my pass'
  set-wifi --mode=wifi-ap --set-channel=6 --ap-ssid=esp-csi-ap
  set-wifi --mode=esp-now-central --peer-mac=aa:bb:cc:dd:ee:ff --ht40=above
  set-wifi --mode=esp-now-fast-collector --set-channel=6
  set-wifi --mode=esp-now-fast-source --set-channel=6

Description:
  Use this command to configure WiFi settings for the CSI collection process.
  - Modes:
      - `station`: Connect to an existing WiFi network.
      - `sniffer`: Monitor WiFi traffic passively.
      - `wifi-ap`: Self-contained softAP CSI collector (pair with `station` on same SSID).
      - `esp-now-central`: Act as a central device in ESP-NOW communication.
      - `esp-now-peripheral`: Act as a peripheral device in ESP-NOW communication.
      - `esp-now-fast-collector` / `esp-now-fast-source`: Asymmetric ESP-NOW simplex for max CSI pps.
  - ESP-NOW options (`--peer-mac`, `--ht40`) apply to all ESP-NOW modes including fast simplex.
  - For AP + STA lab pairs, use `set-protocol --protocol=n` and consider `set-traffic --frequency-hz=4000`."),
        },
        &Item {
            item_type: ItemType::Callback {
                function: start_csi_collect,
                parameters: &[
                    Parameter::NamedValue {
                        parameter_name: "duration",
                        argument_name: "duration",
                        help: Some("Duration of Collection"),
                    },
                ],
            },
            command: "start",
            help: Some("start - Start the CSI collection process.

Usage:
  start [OPTIONS]

Options:
  --duration=<SECONDS>         Specify the duration for the CSI collection process.

Examples:
  start
  start --duration=120
  start --duration=300

Description:
  This command initiates the CSI collection process for a specified duration.
  Before starting, ensure the device is properly configured using the `set-traffic`,
  `set-network`, `set-csi`, and `set-wifi` commands.

  During the collection process:
  - Traffic generation will occur based on the configured parameters (if enabled).
  - CSI data will be collected and printed to the console.
  - After the specified duration, the process will terminate automatically. Otherwise collection runs forever.
  - Press 'q' (or 'Q') at any time to stop collection early."),
        },
        &Item {
            item_type: ItemType::Callback {
                function: show_config,
                parameters: &[],
            },
            command: "show-config",
            help: Some("show-config - Display the current configuration settings.

Usage:
  show-config

Description:
  Prints a summary of every persisted setting:
  - WiFi: mode, channel, station SSID/password, softAP SSID/password/DHCP.
  - Collection: collector/listener role, traffic frequency, PHY rate, TX/RX
    task toggles.
  - CSI Config: chip-specific feature flags (LLTF/HTLTF on classic chips,
    HE/STBC fields on ESP32-C5/C6).

  Use this before `start` to verify the configuration that the next collection
  run will snapshot."),
        },
        &Item {
            item_type: ItemType::Callback {
                function: cli_info,
                parameters: &[],
            },
            command: "info",
            help: Some("info - Print firmware identification metadata.

Usage:
  info

Description:
  Prints a machine-parseable identification block for host-side tooling
  that needs to verify which firmware is running on the device.

Output format:
  ESP-CSI-CLI/<version>
  name=esp-csi-cli-rs
  version=<version>
  chip=<esp32|esp32c3|esp32c5|esp32c6|esp32s3|unknown>
  protocol=<u32>
  features=<comma-separated-list>
  END-INFO

  The same magic line `ESP-CSI-CLI/<version>` is also printed at the top
  of the welcome banner on every reset, so a host can identify the
  firmware passively without sending this command. The `protocol` field
  bumps on any breaking change to this grammar."),
        },
        &Item {
            item_type: ItemType::Callback {
                function: restart_cmd,
                parameters: &[],
            },
            command: "restart",
            help: Some("restart - Reboot the device via a clean software reset.

Usage:
  restart

Description:
  Performs a software reset of the chip. On native-USB boards (the built-in
  USB-Serial-JTAG transport) this drops and re-enumerates the USB device, so
  the serial port may come back as a different /dev/ttyACM* node.

  This is intended to be paired with host tooling that pins each device by its
  stable USB serial number (the factory MAC, reported as the `mac=` field of
  `info` and on the welcome banner) instead of the port path. With that pinning
  in place the re-enumeration is a non-event: the per-device task re-binds to
  the same physical board regardless of the node number it returns as.

  On reboot the firmware re-emits the welcome banner (magic line + `mac=`),
  which a host can grep to confirm the board is back."),
        },
        &Item {
            item_type: ItemType::Callback {
                function: reset_config,
                parameters: &[],
            },
            command: "reset-config",
            help: Some("reset-config - Reset all configurations to their default values.

Usage:
  reset-config

Description:
  Re-initializes the runtime UserConfig with built-in defaults:
  - WiFi mode: Sniffer, channel 1, no station SSID/password.
  - Collection: Collector, traffic frequency 100 Hz.
  - PHY rate: MCS0-LGI; IO tasks: TX + RX both enabled.
  - CSI feature flags: chip default (all enabled / max-detail).

  Use this command if you want to start fresh with the default configuration."),
        },
        &Item {
            item_type: ItemType::Callback {
                function: set_phy_rate,
                parameters: &[
                    Parameter::NamedValue {
                        parameter_name: "rate",
                        argument_name: "rate",
                        help: Some("Wi-Fi PHY rate (e.g. mcs0-lgi, 24m, 54m)"),
                    },
                ],
            },
            command: "set-rate",
            help: Some("set-rate - Set the Wi-Fi PHY rate (ESP-NOW modes only).

Usage:
  set-rate --rate=<rate>

Options:
  --rate=<NAME>   One of: mcs0-lgi (default), mcs1-lgi..mcs7-lgi, mcs0-sgi,
                  1m, 2m, 5m5, 11m, 6m, 9m, 12m, 18m, 24m, 36m, 48m, 54m.

Examples:
  set-rate --rate=mcs0-lgi
  set-rate --rate=24m

Description:
  Selects the Wi-Fi PHY rate used by ESP-NOW central / peripheral nodes.
  Sniffer and station modes derive their rate from the surrounding radio
  configuration and ignore this setting."),
        },
        &Item {
            item_type: ItemType::Callback {
                function: set_protocol_cmd,
                parameters: &[
                    Parameter::NamedValue {
                        parameter_name: "protocol",
                        argument_name: "protocol",
                        help: Some("Wi-Fi PHY protocol: b|g|n|lr|a|ac"),
                    },
                ],
            },
            command: "set-protocol",
            help: Some("set-protocol - Set the Wi-Fi PHY protocol.

Usage:
  set-protocol --protocol=<b|g|n|lr|a|ac>

Options:
  --protocol=<NAME>   One of: b, g, n, lr (default), a, ac.

Examples:
  set-protocol --protocol=lr     # ESP-to-ESP long range (sniffer / ESP-NOW)
  set-protocol --protocol=n      # 802.11n, e.g. station mode against an AP

Description:
  Applied to the node via CSINode::set_protocol at the start of each
  collection run. Previously this was hardcoded per WiFi mode (LR for
  sniffer/ESP-NOW, N for station); it is now an explicit setting.

  Pick the protocol to match your link: LR for maximum range between ESP
  devices, N when associating to a standard AP in station mode. Not every
  part supports every protocol — unsupported values may be rejected by the
  radio at start."),
        },
        &Item {
            item_type: ItemType::Callback {
                function: set_io_tasks_cmd,
                parameters: &[
                    Parameter::NamedValue {
                        parameter_name: "tx",
                        argument_name: "tx",
                        help: Some("TX task: on|off"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "rx",
                        argument_name: "rx",
                        help: Some("RX task: on|off"),
                    },
                ],
            },
            command: "set-io-tasks",
            help: Some("set-io-tasks - Toggle TX and/or RX direction tasks.

Usage:
  set-io-tasks [--tx=<on|off>] [--rx=<on|off>]

Examples:
  set-io-tasks --tx=off          # listener-only node
  set-io-tasks --tx=on --rx=on   # bidirectional (default)

Description:
  Mirrors `IOTaskConfig` in esp-csi-rs. Disabling RX turns the node into a
  pure transmitter (skips the WiFi-callback CSI path); disabling TX turns
  it into a pure receiver (no traffic generation). Both omitted leaves the
  current state untouched."),
        },
        &Item {
            item_type: ItemType::Callback {
                function: set_csi_delivery_cmd,
                parameters: &[
                    Parameter::NamedValue {
                        parameter_name: "mode",
                        argument_name: "mode",
                        help: Some("Delivery: off|callback|async|raw"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "logging",
                        argument_name: "logging",
                        help: Some("Inline log gate: on|off"),
                    },
                ],
            },
            command: "set-csi-delivery",
            help: Some("set-csi-delivery - Switch CSI delivery mode at runtime.

Usage:
  set-csi-delivery [--mode=<off|callback|async|raw>] [--logging=<on|off>]

Options:
  --mode=off        No user delivery. Inline `log_csi` may still run.
  --mode=callback   Dispatch to the registered set_csi_callback hook.
  --mode=async      Queue packets for CSINodeClient::next_csi_packet (default
                    used by the CLI's indefinite collection path).
  --mode=raw        Zero-copy CPU-benchmark fast-path: the WiFi callback returns
                    before building the CSIDataPacket, so no CSI data is
                    delivered or logged. Applies on the next `start` (no q-key
                    stop), and also skips ESP-NOW control-packet ingest.
  --logging=on/off  Toggle the per-packet UART/JTAG `log_csi` gate
                    independently of delivery mode.

Examples:
  set-csi-delivery --mode=async
  set-csi-delivery --mode=off --logging=off
  set-csi-delivery --mode=raw

Description:
  These two flags control the WiFi-callback dispatch path. The two delivery
  paths are mutually exclusive — the callback never pays for both. Use this
  command when you want to flip between paths without re-registering the
  callback or restarting collection."),
        },
        #[cfg(feature = "statistics")]
        &Item {
            item_type: ItemType::Callback {
                function: show_stats,
                parameters: &[],
            },
            command: "show-stats",
            help: Some("show-stats - Print runtime CSI / traffic counters.

Usage:
  show-stats

Description:
  One-shot snapshot of the counters exposed by esp-csi-rs (gated on the
  `statistics` Cargo feature, on by default in this CLI):
  - RX/TX packet totals
  - RX/TX PPS averages
  - RX/TX rate in Hz
  - RX dropped packets
  - ESP-NOW TX queued / confirmed / failed counts

  Counters reset on the start of each new `start` collection."),
        },

    ],
    entry: Some(enter_root),
    exit: None,
};
