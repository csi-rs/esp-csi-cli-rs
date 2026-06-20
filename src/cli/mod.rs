mod cli;
mod serial;
mod cmds;

use cli::{enter_root};
use menu::{Item, ItemType, Menu, Parameter};

use crate::cli::cmds::{
    cli_info, reset_config, set_collection_mode, set_csi, set_csi_delivery_cmd, set_io_tasks_cmd,
    set_log_mode, set_phy_rate, set_traffic, set_wifi, show_config, start_csi_collect,
};
#[cfg(feature = "statistics")]
use crate::cli::cmds::show_stats;
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
                ],
            },
            command: "set-traffic",
            help: Some(
                "set-traffic - Configure traffic-related parameters.

Usage:
  set-traffic [OPTIONS]

Options:
  --frequency-hz=<NUMBER>      Specify the traffic frequencey in Hz (default: 100).

Examples:
  set-traffic --frequency-hz=10

Description:
  This command allows you to configure traffic parameters for the CSI collection process.
  You can enable traffic generation and specify the interval 
  between generated packets. Setting a value of zero disbles traffic generation.",
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
                    Parameter::Named {
                        parameter_name: "disable-lltf",
                        help: Some("Disable LLTF"),
                    },
                    Parameter::Named {
                        parameter_name: "disable-htltf",
                        help: Some("Disable HTLTF"),
                    },
                    Parameter::Named {
                        parameter_name: "disable-stbc-htltf",
                        help: Some("Disable STBC HTLTF"),
                    },
                    Parameter::Named {
                        parameter_name: "disable-ltf-merge",
                        help: Some("Disable LTF Merge"),
                    },
                ],
            },
            command: "set-csi",
            help: Some("set-csi - Configure CSI feature flags.

Usage:
    set-csi [OPTIONS]

    Options:
    --disable-lltf               Disable LLTF CSI configuration (default: enabled).
    --disable-htltf              Disable HTLTF CSI configuration (default: enabled).
    --disable-stbc-htltf         Disable STBC HTLTF CSI configuration (default: enabled).
    --disable-ltf-merge          Disable LTF Merge CSI configuration (default: enabled).

Examples:
    set-csi --disable-lltf --disable-ltf-merge
    set-csi --disable-htltf

Description:
This command allows you to enable or disable specific Channel State Information (CSI) features. 
By default, all CSI features are enabled. Use the options to selectively disable specific
configurations if necessary.

Note:
CSI Configuration is ignored when running in Access Point Mode."),
        },
        #[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
        &Item {
            item_type: ItemType::Callback {
                function: set_csi,
                parameters: &[
                    Parameter::Named {
                        parameter_name: "disable-csi",
                        help: Some("Disable acquisition of CSI"),
                    },
                    Parameter::Named {
                        parameter_name: "disable-csi-legacy",
                        help: Some("Disable acquisition of L-LTF when receiving a 11g PPDU"),
                    },
                    Parameter::Named {
                        parameter_name: "disable-csi-ht20",
                        help: Some("Disable acquisition of HT-LTF when receiving an HT20 PPDU"),
                    },
                    Parameter::Named {
                        parameter_name: "disable-csi-ht40",
                        help: Some("Disable acquisition of HT-LTF when receiving an HT40 PPDU"),
                    },
                    Parameter::Named {
                        parameter_name: "disable-csi-su",
                        help: Some("Disable acquisition of HE-LTF when receiving an HE20 SU PPDU"),
                    },
                    Parameter::Named {
                        parameter_name: "disable-csi-mu",
                        help: Some("Disable acquisition of HE-LTF when receiving an HE20 MU PPDU"),
                    },
                    Parameter::Named {
                        parameter_name: "disable-csi-dcm",
                        help: Some("Disable acquisition of HE-LTF when receiving an HE20 DCM applied PPDU"),
                    },
                    Parameter::Named {
                        parameter_name: "disable-csi-beamformed",
                        help: Some("Disable acquisition of HE-LTF when receiving an HE20 Beamformed applied PPDU"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "csi-he-stbc",
                        argument_name: "csihestbc",
                        help: Some("When receiving an STBC applied HE PPDU 0-3 value"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "val-scale-cfg",
                        argument_name: "valscalecfg",
                        help: Some("Value 0-3"),
                    },
                ],
            },
            command: "set-csi",
            help: Some("set-csi - Configure CSI feature flags.

Usage:
    set-csi [OPTIONS]

    Options:
    --disable-csi               Disable acquisition of CSI (default: enabled)
    --disable-csi-legacy        Disable acquisition of L-LTF when receiving a 11g PPDU (default: enabled)
    --disable-csi-ht20          Disable acquisition of HT-LTF when receiving an HT20 PPDU (default: enabled)
    --disable-csi-ht40          Disable acquisition of HT-LTF when receiving an HT40 PPDU (default: enabled)
    --disable-csi-su            Disable acquisition of HE-LTF when receiving an HE20 SU PPDU (default: enabled)
    --disable-csi-mu            Disable acquisition of HE-LTF when receiving an HE20 MU PPDU (default: enabled)
    --disable-csi-dcm           Disable acquisition of HE-LTF when receiving an HE20 DCM applied PPDU (default: enabled)
    --disable-csi-beamformed    Disable acquisition of HE-LTF when receiving an HE20 Beamformed applied PPDU (default: enabled)
    --csi-he-stbc               When receiving an STBC applied HE PPDU,
                                    0- acquire the complete HE-LTF1
                                    1- acquire the complete HE-LTF2
                                    2- sample evenly among the HE-LTF1 and HE-LTF2
                                    (default: 2)
    --val-scale-cfg             Value 0-3 (default: 2)

Examples:
    set-csi --disable-csi-legacy --csi-he-stbc=1
    set-csi --disable-csi

Description:
This command allows you to enable or disable specific Channel State Information (CSI) features. 
By default, all CSI features are enabled. Use the options to selectively disable specific
configurations if necessary."),
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
  --mode=<station|sniffer|esp-now-central|esp-now-peripheral>   Specify WiFi operation mode (default: sniffer).
  --sta-ssid=<SSID>                                             Set the SSID for the station (default: empty).
  --sta-password=<PASSWORD>                                     Set the password for the station (default: empty).
  --set-channel=<NUMBER>                                        Set the channel (default: 1).
  --peer-mac=<aa:bb:cc:dd:ee:ff>                                ESP-NOW: explicit peer MAC. Switches off automatic
                                                                magic-prefix pairing for per-node source-MAC filtering.
                                                                Pass an empty value to clear (back to auto pairing).
  --ht40=<above|below|none>                                     ESP-NOW: force the per-peer TX PHY to HT40 with the
                                                                given secondary channel (default: none = HT20/legacy).

Examples:
  set-wifi --mode=sniffer
  set-wifi --mode=station --sta-ssid='My WiFi' --sta-password='my pass'
  set-wifi --mode=esp-now-central --peer-mac=aa:bb:cc:dd:ee:ff --ht40=above

Description:
  Use this command to configure WiFi settings for the CSI collection process.
  - Modes:
      - `station`: Connect to an existing WiFi network.
      - `sniffer`: Monitor WiFi traffic passively.
      - `esp-now-central`: Act as a central device in ESP-NOW communication.
      - `esp-now-peripheral`: Act as a peripheral device in ESP-NOW communication.
  - ESP-NOW options (`--peer-mac`, `--ht40`) only take effect in ESP-NOW modes."),
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
  - WiFi: mode, channel, station SSID/password.
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
