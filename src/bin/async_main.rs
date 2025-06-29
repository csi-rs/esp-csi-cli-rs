#![no_std]
#![no_main]

use core::cell::RefCell;
use core::fmt::Write;
use core::u64;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::signal::Signal;
use esp_backtrace as _;
use esp_backtrace as _;
use esp_csi_rs::{config::TrafficType, NetworkArchitechture};
use esp_csi_rs::{CSICollector, WiFiMode};
use esp_hal::timer::timg::TimerGroup;
#[cfg(feature = "esp32")]
use esp_hal::uart::{Config, Uart};
#[cfg(not(feature = "esp32"))]
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::Async;
use esp_println::println;
use esp_wifi::wifi::{Interfaces, WifiController};
use esp_wifi::{init, EspWifiController};
use menu::*;

extern crate alloc;

static CSI_COLLECTOR: Mutex<CriticalSectionRawMutex, RefCell<Option<CSICollector>>> =
    Mutex::new(RefCell::new(None));
static START_SIGNAL: Signal<CriticalSectionRawMutex, Option<u64>> = Signal::new();

#[derive(Default)]
struct Context {
    _inner: u32,
}

#[cfg(not(feature = "esp32"))]
type SerialInterfaceType<'a> = UsbSerialJtag<'a, Async>;
#[cfg(feature = "esp32")]
type SerialInterfaceType<'a> = Uart<'a, Async>;

// CLI Root Menu Struct Initialization
const ROOT_MENU: Menu<SerialInterfaceType, Context> = Menu {
    label: "root",
    items: &[
        &Item {
            item_type: ItemType::Callback {
                function: set_traffic,
                parameters: &[
                    Parameter::Named {
                        parameter_name: "enable",
                        help: Some("Enables Traffic Generation"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "type",
                        argument_name: "type",
                        help: Some("Traffic Type"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "interval",
                        argument_name: "interval",
                        help: Some("Traffic Generation Interval"),
                    },
                ],
            },
            command: "set-traffic",
            help: Some(
                "set-traffic - Configure traffic-related parameters.

Usage:
  set-traffic [OPTIONS]

Options:
  --enable                     Enable traffic generation (default: disabled).
  --type=<icmp|udp>        Set the type of traffic (default: icmp).
  --interval-ms=<NUMBER>       Specify the traffic interval in milliseconds (default: 1000).

Examples:
  set-traffic --enable --type=udp --interval-ms=50
  set-traffic --enable

Description:
  This command allows you to configure traffic parameters for the CSI collection process.
  You can enable traffic generation, set the traffic type, and specify the interval 
  between generated packets.",
            ),
        },
        &Item {
            item_type: ItemType::Callback {
                function: set_network,
                parameters: &[Parameter::NamedValue {
                    parameter_name: "arch",
                    argument_name: "arch",
                    help: Some("Desired Network Architecture"),
                }],
            },
            command: "set-network",
            help: Some("set-network - Configure network architecture settings.

NOTE: Setting the network architechture is only necessary if NTP synchronization is desired

Usage:
  set-network [OPTIONS]

Options:
  --arch=<rsta|rapsta|apsta|sniff>      Define the network architecture (default: sniff).

Examples:
  set-network --arch=rsta
  set-network --arch=apsta

Description:
  This command is used to configure the network architecture for the CSI collection process.
  The architecture can be set to:
    - `rsta`: Internet-based router connected to one station.
    - `rapsta`: Internet-based router connected to Access Point + Station that is Connected to One or More Station(s).
    - `apsta`: Access point connected to one or more stations. No NTP sync perfromed.
    - `sniff`: Standalone device sniffing packets. No NTP sync performed. (default setting).",
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
configurations if necessary."),
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
                        parameter_name: "max-connections",
                        argument_name: "wifimaxconn",
                        help: Some("Specify maxiumum number of allowed connections (if configured as AP)"),
                    },
                    Parameter::Named {
                        parameter_name: "hide-ssid",
                        help: Some("Hide SSID (if configured as AP)"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "ap-ssid",
                        argument_name: "apssid",
                        help: Some("The SSID for the AP"),
                    },
                    Parameter::NamedValue {
                        parameter_name: "ap-password",
                        argument_name: "appassword",
                        help: Some("The password for the AP"),
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
  --mode=<ap|station|sniffer|ap-station>   Specify WiFi operation mode (default: sniffer).
  --max-connections=<NUMBER>               Set the maximum number of AP connections (default: 1).
  --hide-ssid                              Hide the SSID for the AP (default: visible).
  --ap-ssid=<SSID>                         Set the SSID for the AP (default: empty).
  --ap-password=<PASSWORD>                 Set the password for the AP (default: empty).
  --sta-ssid=<SSID>                        Set the SSID for the station (default: empty).
  --sta-password=<PASSWORD>                Set the password for the station (default: empty).
  --set-channel=<NUMBER>                   Set the channel (default: 1).

Examples:
  set-wifi --mode=ap --max-connections=5 --hide-ssid
  set-wifi --mode=station

Description:
  Use this command to configure WiFi settings for the CSI collection process.
  - Modes:
      - `ap`: Access Point mode.
      - `station`: Connect to an existing WiFi network.
      - `sniffer`: Monitor WiFi traffic passively.
      - `ap-station`: Simultaneously act as an AP and connect to another network.

  - Use `--hide-ssid` to make the SSID of an AP invisible to scanning devices."),
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

NOTE: If configured as a Station, make sure there is already a running/started Access Point

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
  - WiFi settings: Mode set to Sniffer, maximum AP connections set to 1, SSID visible.

  Use this command if you want to start fresh with the default configuration."),
        },

    ],
    entry: Some(enter_root),
    exit: None,
};

fn enter_root(
    _menu: &Menu<SerialInterfaceType, Context>,
    interface: &mut SerialInterfaceType,
    _context: &mut Context,
) {
    interface
        .write_str("******* Welcome to the CSI Collection CLI utility! *******")
        .unwrap();
    interface.write_str("\n").unwrap();
    interface
        .write_str(
            "Available Commands:
    set-traffic         Configure traffic-related parameters (e.g. type, interval).
    set-network         Configure network architecture settings.
    set-csi             Configure CSI feature flags (e.g., LLTF, HTLTF).
    set-wifi            Configure WiFi settings (e.g., mode, SSID visibility).
    start               Start the CSI collection process with a defined duration.
    show-config         Display the current configuration settings.
    reset-config        Reset all configurations to their default values.
    help                Display this help menu or details for a specific command.

    For more information on a specific command, type:
    help <command>

    Example:
    help set-traffic",
        )
        .unwrap();
    // interface.flush().unwrap();
}

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // Initalize ESP device and acquire peripherals
    let config = esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Allocate heap space
    esp_alloc::heap_allocator!(size: 72 * 1024);

    // Initalize Embassy
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timg1.timer0);

    // Instantiate peripherals for EspWifiController
    let timer = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
    let mut rng = esp_hal::rng::Rng::new(peripherals.RNG);
    let seed = rng.random();

    // Initialize ESP WiFi Controller
    let init = &*mk_static!(
        EspWifiController<'static>,
        init(timer.timer0, rng, peripherals.RADIO_CLK).unwrap()
    );

    // Instantiate WiFi controller and interfaces
    let (controller, interfaces) = esp_wifi::wifi::new(&init, peripherals.WIFI).unwrap();

    // Create an instance for the CSI Collector
    let csi_config = CSICollector::new_with_defaults();

    // Pass Collector Instance to Global Context
    CSI_COLLECTOR.lock(|config| {
        config.replace(Some(csi_config));
    });

    // Spawn the CSI Collection Task
    spawner
        .spawn(csi_collector(controller, interfaces, seed as u64, spawner))
        .unwrap();

    // Instantiate Serial Interface for CLI host communication
    #[cfg(not(feature = "esp32"))]
    let serial = UsbSerialJtag::new(peripherals.USB_DEVICE).into_async();

    #[cfg(feature = "esp32")]
    let serial = {
        let (tx_pin, rx_pin) = (peripherals.GPIO1, peripherals.GPIO3);
        let config = Config::default();
        Uart::new(peripherals.UART0, config)
            .unwrap()
            .with_tx(tx_pin)
            .with_rx(rx_pin)
            .into_async()
    };

    // Create a buffer to store CLI input
    let mut clibuf = [0u8; 128];
    // Instantiate Context placeholder
    let mut context = Context::default();
    // Instantiate CLI runner with root menu, buffer, and serial
    let mut runner = Runner::new(ROOT_MENU, &mut clibuf, serial, &mut context);

    loop {
        // Create single element buffer for serial characters
        let mut buf = [0_u8; 1];
        embedded_io_async::Read::read(&mut runner.interface, &mut buf)
            .await
            .unwrap();
        // Pass read byte to CLI runner for processing
        runner.input_byte(buf[0], &mut context);
    }
}

#[embassy_executor::task]
async fn csi_collector(
    controller: WifiController<'static>,
    interface: Interfaces<'static>,
    seed: u64,
    spawner: Spawner,
) {
    // Wait for first start signal to kick off collection activity
    let mut interval = START_SIGNAL.wait().await;

    // Obtain copy from CSI collector in global context
    let mut collector =
        CSI_COLLECTOR.lock(|collector| (collector.borrow().as_ref().unwrap().clone()));

    // Initalize CSI collector
    match collector.init(controller, interface, seed, &spawner) {
        Ok(_) => {}
        Err(_e) => {
            println!("Error Initializing CSI Collector");
        }
    }

    loop {
        // Start Collection
        let _ = collector.start(interval);
        // Reset Start Signal Once collection completes
        START_SIGNAL.reset();
        // Update Interval & Start Again when signalled
        interval = START_SIGNAL.wait().await;
        // Obtain new configuration before starting again
        collector = CSI_COLLECTOR.lock(|collector| (collector.borrow().as_ref().unwrap().clone()));
    }
}

fn set_traffic<'a>(
    _menu: &Menu<SerialInterfaceType, Context>,
    item: &Item<SerialInterfaceType, Context>,
    args: &[&str],
    serial: &mut SerialInterfaceType,
    _context: &mut Context,
) {
    let traffic_en = argument_finder(item, args, "enable");
    let traffic_type = argument_finder(item, args, "type");
    let traffic_interval = argument_finder(item, args, "interval");

    match traffic_en {
        Ok(_str) => CSI_COLLECTOR.lock(|config| {
            config.borrow_mut().as_mut().unwrap().traffic_enabled = true;
        }),
        Err(_) => (),
    }
    match traffic_type {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap() {
                    "icmp" => CSI_COLLECTOR.lock(|config| {
                        config
                            .borrow_mut()
                            .as_mut()
                            .unwrap()
                            .traffic_config
                            .traffic_type = TrafficType::ICMPPing;
                    }),
                    "udp" => CSI_COLLECTOR.lock(|config| {
                        config
                            .borrow_mut()
                            .as_mut()
                            .unwrap()
                            .traffic_config
                            .traffic_type = TrafficType::UDP;
                    }),
                    _ => writeln!(serial, "Invalid Traffic Type").unwrap(),
                }
            }
        }
        Err(_) => (),
    }

    match traffic_interval {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap().parse::<u64>() {
                    Ok(interval) => CSI_COLLECTOR.lock(|config| {
                        config
                            .borrow_mut()
                            .as_mut()
                            .unwrap()
                            .traffic_config
                            .traffic_interval_ms = interval
                    }),
                    Err(_) => writeln!(serial, "Invalid Interval").unwrap(),
                }
            }
        }
        Err(_) => (),
    }

    writeln!(serial, "\nUpdated Traffic Configuration:\n").unwrap();
    CSI_COLLECTOR.lock(|config| {
        writeln!(
            serial,
            "Traffic Enabled: {}",
            config.borrow().as_ref().unwrap().traffic_enabled
        )
        .unwrap();
        writeln!(
            serial,
            "Traffic Type: {:?}",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .traffic_config
                .traffic_type
        )
        .unwrap();
        writeln!(
            serial,
            "Traffic Interval: {}ms",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .traffic_config
                .traffic_interval_ms
        )
        .unwrap();
    });
}

fn set_network<'a>(
    _menu: &Menu<SerialInterfaceType, Context>,
    item: &Item<SerialInterfaceType, Context>,
    args: &[&str],
    serial: &mut SerialInterfaceType,
    _context: &mut Context,
) {
    let arch = argument_finder(item, args, "arch");

    match arch {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap() {
                    "rsta" => CSI_COLLECTOR.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().net_arch =
                            NetworkArchitechture::RouterStation;
                    }),
                    "rapsta" => CSI_COLLECTOR.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().net_arch =
                            NetworkArchitechture::RouterAccessPointStation;
                    }),
                    "apsta" => CSI_COLLECTOR.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().net_arch =
                            NetworkArchitechture::AccessPointStation;
                    }),
                    "sniff" => CSI_COLLECTOR.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().net_arch =
                            NetworkArchitechture::Sniffer;
                    }),
                    _ => writeln!(serial, "Invalid Network Type").unwrap(),
                }
            }
        }
        Err(_) => (),
    }

    CSI_COLLECTOR.lock(|config| {
        writeln!(
            serial,
            "\nUpdated Network Architechture Configuration to {:?}",
            config.borrow().as_ref().unwrap().net_arch
        )
        .unwrap();
    });
}

#[cfg(feature = "esp32c6")]
fn set_csi<'a>(
    _menu: &Menu<SerialInterfaceType, Context>,
    item: &Item<SerialInterfaceType, Context>,
    args: &[&str],
    serial: &mut SerialInterfaceType,
    _context: &mut Context,
) {
    let disable_csi = argument_finder(item, args, "disable-csi");
    let disable_csi_legacy = argument_finder(item, args, "disable-csi-legacy");
    let disable_csi_ht20 = argument_finder(item, args, "disable-csi-ht20");
    let disable_csi_ht40 = argument_finder(item, args, "disable-csi-ht40");
    let disable_csi_su = argument_finder(item, args, "disable-csi-su");
    let disable_csi_mu = argument_finder(item, args, "disable-csi-mu");
    let disable_csi_dcm = argument_finder(item, args, "disable-csi-dcm");
    let disable_csi_beamformed = argument_finder(item, args, "disable-csi-beamformed");
    let csi_he_stbc = argument_finder(item, args, "csi-he-stbc");
    let val_scale_cfg = argument_finder(item, args, "val-scale-cfg");

    match disable_csi {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR
                    .lock(|config| config.borrow_mut().as_mut().unwrap().csi_config.enable = 0)
            }
        }
        Err(_) => (),
    }
    match disable_csi_legacy {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .acquire_csi_legacy = 0;
                })
            }
        }
        Err(_) => (),
    }
    match disable_csi_ht20 {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .acquire_csi_ht20 = 0;
                })
            }
        }
        Err(_) => (),
    }
    match disable_csi_ht40 {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .acquire_csi_ht40 = 0;
                })
            }
        }
        Err(_) => (),
    }
    match disable_csi_su {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .acquire_csi_su = 0;
                })
            }
        }
        Err(_) => (),
    }
    match disable_csi_mu {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .acquire_csi_mu = 0;
                })
            }
        }
        Err(_) => (),
    }
    match disable_csi_dcm {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .acquire_csi_dcm = 0;
                })
            }
        }
        Err(_) => (),
    }
    match disable_csi_beamformed {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .acquire_csi_beamformed = 0;
                })
            }
        }
        Err(_) => (),
    }
    match csi_he_stbc {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap().parse::<u32>() {
                    Ok(val) => CSI_COLLECTOR.lock(|config| {
                        config
                            .borrow_mut()
                            .as_mut()
                            .unwrap()
                            .csi_config
                            .acquire_csi_he_stbc = val;
                    }),
                    Err(_) => writeln!(serial, "Invalid Max Connections").unwrap(),
                }
            }
        }
        Err(_) => (),
    }
    match val_scale_cfg {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap().parse::<u32>() {
                    Ok(val) => CSI_COLLECTOR.lock(|config| {
                        config
                            .borrow_mut()
                            .as_mut()
                            .unwrap()
                            .csi_config
                            .val_scale_cfg = val;
                    }),
                    Err(_) => writeln!(serial, "Invalid Max Connections").unwrap(),
                }
            }
        }
        Err(_) => (),
    }

    writeln!(serial, "\nUpdated CSI Configuration:\n").unwrap();
    CSI_COLLECTOR.lock(|config| {
        writeln!(
            serial,
            "Acquire CSI: {}",
            config.borrow().as_ref().unwrap().csi_config.enable
        )
        .unwrap();
        writeln!(
            serial,
            "Acquire Legacy CSI: {}",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_legacy
        )
        .unwrap();
        writeln!(
            serial,
            "Acquire HT20: {}",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_ht20
        )
        .unwrap();
        writeln!(
            serial,
            "Acquire HT40: {}",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_ht40
        )
        .unwrap();
        writeln!(
            serial,
            "Acquire HE20 SU: {}",
            config.borrow().as_ref().unwrap().csi_config.acquire_csi_su
        )
        .unwrap();
        writeln!(
            serial,
            "Acquire HE20 MU: {}",
            config.borrow().as_ref().unwrap().csi_config.acquire_csi_mu
        )
        .unwrap();
        writeln!(
            serial,
            "Acquire HE20 DCM: {}",
            config.borrow().as_ref().unwrap().csi_config.acquire_csi_dcm
        )
        .unwrap();
        writeln!(
            serial,
            "Acquire HE20 Beamformed: {}",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_beamformed
        )
        .unwrap();
        writeln!(
            serial,
            "STBC HE: {}",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_he_stbc
        )
        .unwrap();
        writeln!(
            serial,
            "Scale Value: {}",
            config.borrow().as_ref().unwrap().csi_config.val_scale_cfg
        )
        .unwrap();
    });
}

#[cfg(not(feature = "esp32c6"))]
fn set_csi<'a>(
    _menu: &Menu<SerialInterfaceType, Context>,
    item: &Item<SerialInterfaceType, Context>,
    args: &[&str],
    serial: &mut SerialInterfaceType,
    _context: &mut Context,
) {
    let disable_lltf = argument_finder(item, args, "disable-lltf");
    let disable_htltf = argument_finder(item, args, "disable-htltf");
    let disable_stbc_htltf = argument_finder(item, args, "disable-stbc-htltf");
    let disable_ltf_merge = argument_finder(item, args, "disable-ltf-merge");

    match disable_lltf {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .lltf_enabled = false;
                })
            }
        }
        Err(_) => (),
    }
    match disable_htltf {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .htltf_enabled = false;
                })
            }
        }
        Err(_) => (),
    }
    match disable_stbc_htltf {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .stbc_htltf2_enabled = false;
                })
            }
        }
        Err(_) => (),
    }
    match disable_ltf_merge {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .ltf_merge_enabled = false;
                })
            }
        }
        Err(_) => (),
    }

    writeln!(serial, "\nUpdated CSI Configuration:\n").unwrap();
    CSI_COLLECTOR.lock(|config| {
        writeln!(
            serial,
            "LLTF Enabled: {}",
            config.borrow().as_ref().unwrap().csi_config.lltf_enabled
        )
        .unwrap();
        writeln!(
            serial,
            "HTLTF Enabled: {}",
            config.borrow().as_ref().unwrap().csi_config.htltf_enabled
        )
        .unwrap();
        writeln!(
            serial,
            "STBC HTLTF Enabled: {}",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .stbc_htltf2_enabled
        )
        .unwrap();
        writeln!(
            serial,
            "LTF Merge Enabled: {}",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .ltf_merge_enabled
        )
        .unwrap();
    });
}

fn set_wifi<'a>(
    _menu: &Menu<SerialInterfaceType, Context>,
    item: &Item<SerialInterfaceType, Context>,
    args: &[&str],
    serial: &mut SerialInterfaceType,
    _context: &mut Context,
) {
    let mode = argument_finder(item, args, "mode");
    let max_connections = argument_finder(item, args, "max-connections");
    let hide_ssid = argument_finder(item, args, "hide-ssid");
    let ap_ssid = argument_finder(item, args, "ap-ssid");

    let ap_password = argument_finder(item, args, "ap-password");
    let sta_ssid = argument_finder(item, args, "sta-ssid");
    let sta_password = argument_finder(item, args, "sta-password");
    let set_channel = argument_finder(item, args, "setchannel");

    match mode {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap() {
                    "ap" => CSI_COLLECTOR.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().op_mode = WiFiMode::AccessPoint;
                    }),
                    "station" => CSI_COLLECTOR.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().op_mode = WiFiMode::Station;
                    }),
                    "sniffer" => CSI_COLLECTOR.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().op_mode = WiFiMode::Sniffer;
                    }),
                    "ap-station" => CSI_COLLECTOR.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().op_mode =
                            WiFiMode::AccessPointStation;
                    }),
                    _ => writeln!(serial, "Invalid WiFi Mode").unwrap(),
                }
            }
        }
        Err(_) => (),
    }
    match max_connections {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap().parse::<u16>() {
                    Ok(max_conn) => CSI_COLLECTOR.lock(|config| {
                        config
                            .borrow_mut()
                            .as_mut()
                            .unwrap()
                            .wifi_config
                            .max_connections = max_conn;
                    }),
                    Err(_) => writeln!(serial, "Invalid Max Connections").unwrap(),
                }
            }
        }
        Err(_) => (),
    }
    match set_channel {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap().parse::<u8>() {
                    Ok(chan) => CSI_COLLECTOR.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().wifi_config.channel = chan;
                    }),
                    Err(_) => writeln!(serial, "Invalid Max Connections").unwrap(),
                }
            }
        }
        Err(_) => (),
    }
    match hide_ssid {
        Ok(str) => {
            if str.is_some() {
                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .wifi_config
                        .ssid_hidden = true;
                })
            }
        }
        Err(_) => (),
    }
    match ap_ssid {
        Ok(str) => {
            if let Some(s) = str {
                let str_w_space = s.replace("_", " ");
                // Convert the `mod_str` into a `heapless::String<32>`
                let mut hpls_str_w_space = heapless::String::<32>::new();
                hpls_str_w_space.push_str(&str_w_space).unwrap(); // Ensure it fits within the capacity

                CSI_COLLECTOR.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().wifi_config.ap_ssid =
                        hpls_str_w_space.try_into().unwrap();
                });
            }
        }
        Err(_) => (),
    }
    match ap_password {
        Ok(str) => {
            if let Some(s) = str {
                let str_w_space = s.replace("_", " ");
                // Convert the `mod_str` into a `heapless::String<32>`
                let mut hpls_str_w_space = heapless::String::<64>::new();
                hpls_str_w_space.push_str(&str_w_space).unwrap(); // Ensure it fits within the capacity

                CSI_COLLECTOR.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .wifi_config
                        .ap_password = hpls_str_w_space.try_into().unwrap();
                });
            }
        }
        Err(_) => (),
    }
    match sta_ssid {
        Ok(str) => {
            if let Some(s) = str {
                let str_w_space = s.replace("_", " ");
                // Convert the `mod_str` into a `heapless::String<32>`
                let mut hpls_str_w_space = heapless::String::<32>::new();
                hpls_str_w_space.push_str(&str_w_space).unwrap(); // Ensure it fits within the capacity

                CSI_COLLECTOR.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().wifi_config.ssid =
                        hpls_str_w_space.try_into().unwrap();
                });
            }
        }
        Err(_) => (),
    }
    match sta_password {
        Ok(str) => {
            if let Some(s) = str {
                let str_w_space = s.replace("_", " ");
                // Convert the `mod_str` into a `heapless::String<32>`
                let mut hpls_str_w_space = heapless::String::<64>::new();
                hpls_str_w_space.push_str(&str_w_space).unwrap(); // Ensure it fits within the capacity

                CSI_COLLECTOR.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().wifi_config.password =
                        hpls_str_w_space.try_into().unwrap();
                });
            }
        }
        Err(_) => (),
    }

    writeln!(serial, "\nUpdated WiFi Configuration:\n").unwrap();
    CSI_COLLECTOR.lock(|config| {
        writeln!(
            serial,
            "WiFi Operation Mode: {:?}",
            config.borrow().as_ref().unwrap().op_mode
        )
        .unwrap();
        writeln!(
            serial,
            "WiFi Channel: {:?}",
            config.borrow().as_ref().unwrap().wifi_config.channel
        )
        .unwrap();
        writeln!(
            serial,
            "Station WiFi Settings:\nSSID: '{}', Password: '{}'",
            config.borrow().as_ref().unwrap().wifi_config.ssid,
            config.borrow().as_ref().unwrap().wifi_config.password,
        )
        .unwrap();
        writeln!(
            serial,
            "Access Point WiFi Settings:\nSSID: '{}', Password: '{}', SSID Hidden: {}, Max Connections: {}",
            config.borrow().as_ref().unwrap().wifi_config.ap_ssid,
            config.borrow().as_ref().unwrap().wifi_config.ap_password,
            config.borrow().as_ref().unwrap().wifi_config.ssid_hidden,
            config.borrow().as_ref().unwrap().wifi_config.max_connections,
        )
        .unwrap();
    });
}

fn start_csi_collect<'a>(
    _menu: &Menu<SerialInterfaceType, Context>,
    item: &Item<SerialInterfaceType, Context>,
    args: &[&str],
    serial: &mut SerialInterfaceType,
    _context: &mut Context,
) {
    let duration = argument_finder(item, args, "duration");
    match duration {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap().parse::<u64>() {
                    Ok(interval) => START_SIGNAL.signal(Some(interval)),
                    Err(_) => writeln!(serial, "Invalid Duration").unwrap(),
                }
            } else {
                // Run for one week if no value provided
                // 604800 seconds is equivalent to one week
                START_SIGNAL.signal(None);
                println!("Running Forever");
            }
        }
        Err(_) => (),
    }
}

fn show_config<'a>(
    _menu: &Menu<SerialInterfaceType, Context>,
    _item: &Item<SerialInterfaceType, Context>,
    _args: &[&str],
    serial: &mut SerialInterfaceType,
    _context: &mut Context,
) {
    writeln!(serial, "\nTraffic Settings:").unwrap();
    CSI_COLLECTOR.lock(|config| {
        writeln!(
            serial,
            "Traffic Enabled: {}",
            config.borrow().as_ref().unwrap().traffic_enabled
        )
        .unwrap();
        writeln!(
            serial,
            "Traffic Type: {:?}",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .traffic_config
                .traffic_type
        )
        .unwrap();
        writeln!(
            serial,
            "Traffic Interval: {}ms",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .traffic_config
                .traffic_interval_ms
        )
        .unwrap();
        writeln!(serial, "\nNetwork Architecture Settings:").unwrap();
        writeln!(
            serial,
            "Network Architecture: {:?}",
            config.borrow().as_ref().unwrap().net_arch
        )
        .unwrap();
        writeln!(serial, "\nCSI Settings:").unwrap();
        #[cfg(not(feature = "esp32c6"))]
        writeln!(
            serial,
            "CSI Feature Flags: LLTF: {}, HTLTF: {}, STBC HTLTF: {}, LTF Merge: {}, Channel Filter: {}",
            config.borrow().as_ref().unwrap().csi_config.lltf_enabled,
            config.borrow().as_ref().unwrap().csi_config.htltf_enabled,
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .stbc_htltf2_enabled,
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .ltf_merge_enabled,
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .channel_filter_enabled,
        )
        .unwrap();
        #[cfg(feature = "esp32c6")]
        writeln!(
            serial,
            "CSI Feature Flags: CSI Enable: {}, CSI Legacy: {}, HT20: {}, HT40: {}, HE20 SU: {}, HE20 MU: {}, HE20 DCM: {}, HE20 Beamformed: {}, HE STBC: {}, Scale Value: {}",
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .enable,
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_legacy,
            config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_ht20,
                config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_ht40,
                config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_su,
                config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_mu,
                config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_dcm,
                config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_beamformed,
                config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .acquire_csi_he_stbc,
                config
                .borrow()
                .as_ref()
                .unwrap()
                .csi_config
                .val_scale_cfg,
        )
        .unwrap();
        writeln!(serial, "\nWiFi Settings:").unwrap();
        writeln!(
            serial,
            "WiFi Operation Mode: {:?}",
            config.borrow().as_ref().unwrap().op_mode,
        )
        .unwrap();
        writeln!(
            serial,
            "Station WiFi Settings:\nSSID: '{}', Password: '{}'",
            config.borrow().as_ref().unwrap().wifi_config.ssid,
            config.borrow().as_ref().unwrap().wifi_config.password,
        )
        .unwrap();
        writeln!(
            serial,
            "Access Point WiFi Settings:\nSSID: '{}', Password: '{}', SSID Hidden: {}, Max Connections: {}",
            config.borrow().as_ref().unwrap().wifi_config.ap_ssid,
            config.borrow().as_ref().unwrap().wifi_config.ap_password,
            config.borrow().as_ref().unwrap().wifi_config.ssid_hidden,
            config.borrow().as_ref().unwrap().wifi_config.max_connections,
        )
        .unwrap();
    });
}

fn reset_config<'a>(
    _menu: &Menu<SerialInterfaceType, Context>,
    _item: &Item<SerialInterfaceType, Context>,
    _args: &[&str],
    serial: &mut SerialInterfaceType,
    _context: &mut Context,
) {
    CSI_COLLECTOR.lock(|config| {
        let default_config = CSICollector::new_with_defaults();
        config.replace(Some(default_config));
    });
    writeln!(serial, "\nConfiguration Reset to Default Values\n").unwrap();
}
