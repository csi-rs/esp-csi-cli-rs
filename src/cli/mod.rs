mod cli;
mod serial;
mod cmds;

use cli::{enter_root};
use menu::{Item, ItemType, Menu, Parameter};

#[cfg(feature = "esp32c6")]
use crate::cli::cmds::set_csi;
use crate::cli::cmds::{reset_config, set_traffic, set_wifi, show_config};
pub use crate::cli::serial::{SerialInterface, is_jtag};

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
  --frequency-hz=<NUMBER>      Specify the traffic frequencey in Hz (default: 0).

Examples:
  set-traffic --frequency-hz=10

Description:
  This command allows you to configure traffic parameters for the CSI collection process.
  You can enable traffic generation and specify the interval 
  between generated packets. Setting a value of zero disbles traffic generation.",
            ),
        },
        #[cfg(not(feature = "esp32c6"))]
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
        #[cfg(feature = "esp32c6")]
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
                ],
            },
            command: "set-wifi",
            help: Some("set-wifi - Configure WiFi settings.

Usage:
  set-wifi [OPTIONS]

IMPORTANT: If your SSID or PASSWORD contains spaces, replace them with underscores.

Options:
  --mode=<station|sniffer|esp-now-central|esp-now-peripheral>   Specify WiFi operation mode (default: sniffer).
  --sta-ssid=<SSID>                                             Set the SSID for the station (default: empty).
  --sta-password=<PASSWORD>                                     Set the password for the station (default: empty).
  --set-channel=<NUMBER>                                        Set the channel (default: 1).

Examples:
  set-wifi --mode=sniffer
  set-wifi --mode=station

Description:
  Use this command to configure WiFi settings for the CSI collection process.
  - Modes:
      - `station`: Connect to an existing WiFi network.
      - `sniffer`: Monitor WiFi traffic passively.
      - `esp-now-central`: Act as a central device in ESP-NOW communication.
      - `esp-now-peripheral`: Act as a peripheral device in ESP-NOW communication."),
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
  - After the specified duration, the process will terminate automatically. Otherwise collection runs forever."),
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

Examples:
  show-config

Description:
  Use this command to display the current configuration for all parameters, including:
  - Traffic settings (enabled/disabled, type, interval).
  - Network architecture (star, mesh, or none).
  - CSI feature flags (enabled/disabled for LLTF, HTLTF, STBC HTLTF, LTF Merge).
  - WiFi settings (mode, maximum connections, SSID visibility).

  The output provides a summary of all settings, allowing you to review and verify configurations
  before starting the CSI collection process."),
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

Examples:
  reset-config

Description:
  This command resets all configurations to their default values:
  - Traffic settings: Disabled, type set to ICMP, interval set to 100ms.
  - Network architecture: Sniffer.
  - CSI feature flags: All enabled (LLTF, HTLTF, STBC HTLTF, LTF Merge).
  - WiFi settings: Mode set to Sniffer, maximum AP connections set to 1.

  Use this command if you want to start fresh with the default configuration."),
        },

    ],
    entry: Some(enter_root),
    exit: None,
};
