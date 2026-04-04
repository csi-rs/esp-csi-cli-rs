#![no_std]
#![no_main]

mod cli;
mod config;

use crate::config::{UserConfig, DONE_SIGNAL, IS_COLLECTING, START_SIGNAL, USER_CONFIG};
#[cfg(any(feature = "esp32c3", feature = "esp32c6", feature = "esp32s3"))]
use cli::is_jtag;
use cli::{Context, ROOT_MENU};
#[cfg(all(
    feature = "auto",
    any(feature = "esp32c3", feature = "esp32c6", feature = "esp32s3")
))]
use cli::SerialInterface;
use core::sync::atomic::Ordering;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::select::{select, Either};
use embedded_io::Write;
use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_csi_rs::logging::logging::{init_logger, LogMode};
use esp_csi_rs::{
    CSINode, CSINodeClient, CSINodeHardware, CentralOpMode, EspNowConfig, Node, PeripheralOpMode,
    WifiSnifferConfig, WifiStationConfig,
};
use esp_hal::timer::timg::TimerGroup;
#[cfg(any(feature = "auto", feature = "uart"))]
use esp_hal::uart::Uart;
#[cfg(all(
    any(feature = "auto", feature = "jtag-serial"),
    any(feature = "esp32c3", feature = "esp32c6", feature = "esp32s3")
))]
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_radio::wifi::{AuthMethod, ClientConfig, Interfaces, WifiController};
use menu::*;

esp_app_desc!();

extern crate alloc;
use alloc::string::ToString;

static WIFI_CONTROLLER: static_cell::StaticCell<WifiController<'static>> =
    static_cell::StaticCell::new();

/// WiFi/radio operating mode selected by the user via `set-wifi --mode`.
///
/// This determines how the underlying [`esp_csi_rs::CSINode`] is constructed
/// and which `esp-radio` interfaces are activated during a collection run.
#[derive(Debug, Clone)]
enum NodeMode {
    /// Passively monitors all WiFi traffic on the configured channel.
    WifiSniffer,
    /// Connects to an existing WiFi network as a station.
    WifiStation,
    /// Acts as the central (initiating) device in an ESP-NOW pair.
    EspNowCentral,
    /// Acts as the peripheral (responding) device in an ESP-NOW pair.
    EspNowPeripheral,
}

/// Helper macro that stores a value in a static [`static_cell::StaticCell`] and returns
/// a `&'static mut` reference to it. Required to pass owned peripherals into Embassy tasks.
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    // Initalize ESP device and acquire peripherals
    let config = esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Allocate heap space
    esp_alloc::heap_allocator!(size: 61 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    #[cfg(any(feature = "esp32c2", feature = "esp32c3", feature = "esp32c6"))]
    {
        let sw_interrupt =
            esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
        esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);
    }
    #[cfg(not(any(feature = "esp32c2", feature = "esp32c3", feature = "esp32c6")))]
    esp_rtos::start(timg0.timer0);

    // Initialize ESP radio Controller
    let radio_init = mk_static!(
        esp_radio::Controller<'static>,
        esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller")
    );

    let mut config_radio = esp_radio::wifi::Config::default();
    config_radio = config_radio.with_power_save_mode(esp_radio::wifi::PowerSaveMode::None);
    let (wifi_controller, interfaces) =
        esp_radio::wifi::new(radio_init, peripherals.WIFI, config_radio)
            .expect("Failed to initialize Wi-Fi controller");

    let controller = WIFI_CONTROLLER.init(wifi_controller);

    // Create an instance for User Configurations
    let user_config = UserConfig::new();

    // Pass User Config Instance to Global Context
    USER_CONFIG.lock(|config| {
        config.replace(Some(user_config));
    });

    // Initialize the CSI data logger
    init_logger(spawner, LogMode::ArrayList);

    // Spawn the CSI Collection Task
    spawner
        .spawn(csi_collection(interfaces, controller))
        .unwrap();

    // Create a buffer to store CLI input
    let mut clibuf = [0u8; 256];
    // Instantiate Context placeholder
    let mut context = Context::default();

    let serial = {
        // ESP32: Always UART
        #[cfg(feature = "esp32")]
        {
            Uart::new(peripherals.UART0, esp_hal::uart::Config::default())
                .unwrap()
                .into_async()
        }

        // Forced JTAG
        #[cfg(all(
            feature = "jtag-serial",
            any(feature = "esp32c3", feature = "esp32c6", feature = "esp32s3")
        ))]
        {
            UsbSerialJtag::new(peripherals.USB_DEVICE).into_async()
        }

        // ESP32-C2: no USB Serial/JTAG peripheral, always use UART.
        #[cfg(all(feature = "esp32c2", feature = "jtag-serial"))]
        {
            Uart::new(peripherals.UART0, esp_hal::uart::Config::default())
                .unwrap()
                .into_async()
        }

        // Forced UART
        #[cfg(feature = "uart")]
        {
            Uart::new(peripherals.UART0, esp_hal::uart::Config::default())
                .unwrap()
                .into_async()
        }

        // Runtime Auto-Detection
        #[cfg(all(
            any(feature = "esp32c3", feature = "esp32c6", feature = "esp32s3"),
            feature = "auto"
        ))]
        {
            if is_jtag() {
                SerialInterface::UsbJtag(
                    UsbSerialJtag::new(peripherals.USB_DEVICE).into_async(),
                )
            } else {
                SerialInterface::Uart(
                    Uart::new(peripherals.UART0, esp_hal::uart::Config::default())
                        .unwrap()
                        .into_async(),
                )
            }
        }

        // ESP32-C2: auto mode falls back to UART because USB Serial/JTAG is unavailable.
        #[cfg(all(feature = "esp32c2", feature = "auto"))]
        {
            Uart::new(peripherals.UART0, esp_hal::uart::Config::default())
                .unwrap()
                .into_async()
        }
    };

    // Instantiate CLI runner with root menu, buffer, and serial
    let mut runner = Runner::new(ROOT_MENU, &mut clibuf, serial, &mut context);

    loop {
        let mut buf = [0_u8; 1];

        if IS_COLLECTING.load(Ordering::Relaxed) {
            // Erase the spurious "> " the menu crate printed after the command returned
            Write::write_all(&mut runner.interface, b"\r\x1b[2K").ok();
            // CLI locked during collection: discard serial input until DONE_SIGNAL
            loop {
                match select(
                    embedded_io_async::Read::read(&mut runner.interface, &mut buf),
                    DONE_SIGNAL.wait(),
                )
                .await
                {
                    Either::First(_) => {}      // discard input
                    Either::Second(_) => break, // collection ended
                }
            }
            IS_COLLECTING.store(false, Ordering::Relaxed);
            // \r       — move to start of line (overwrites the spurious "> " the menu crate printed)
            // \x1b[2K  — ANSI: erase the entire current line
            Write::write_all(
                &mut runner.interface,
                b"\r\x1b[2KCollection complete.\r\n> ",
            )
            .ok();
        } else {
            // Normal CLI mode
            embedded_io_async::Read::read(&mut runner.interface, &mut buf)
                .await
                .unwrap();
            runner.input_byte(buf[0], &mut context);
        }
    }
}

#[embassy_executor::task]
/// Background Embassy task responsible for driving CSI data collection.
///
/// # Lifecycle
/// 1. Waits on [`START_SIGNAL`] for a `Option<u64>` duration sent by the CLI `start` command.
/// 2. Snapshots [`USER_CONFIG`] to get the current node settings.
/// 3. Constructs and runs a [`CSINode`] according to those settings.
///    - `Some(secs)` → [`CSINode::run_duration`] (prints internally).
///    - `None` → [`CSINode::run`] joined with a continuous print loop.
/// 4. Signals [`DONE_SIGNAL`] to unlock the CLI in the main loop.
///
/// This task runs for the lifetime of the application and restarts the cycle
/// on every subsequent `start` command.
async fn csi_collection(
    mut interfaces: Interfaces<'static>,
    controller: &'static mut WifiController<'static>,
) {
    loop {
        // Wait for a start signal from the CLI
        let duration = START_SIGNAL.wait().await;
        START_SIGNAL.reset();

        // Snapshot the current user configuration
        let user_config = USER_CONFIG.lock(|c| c.borrow().as_ref().unwrap().clone());

        // Map NodeMode → esp-csi-rs Node + operation mode
        let node_kind = match user_config.node_mode {
            NodeMode::WifiSniffer => {
                Node::Peripheral(PeripheralOpMode::WifiSniffer(WifiSnifferConfig::default()))
            }
            NodeMode::WifiStation => {
                let client_config = ClientConfig::default()
                    .with_ssid(user_config.sta_ssid.as_str().to_string())
                    .with_password(user_config.sta_password.as_str().to_string())
                    .with_auth_method(AuthMethod::Wpa2Personal);
                Node::Central(CentralOpMode::WifiStation(WifiStationConfig {
                    client_config,
                }))
            }
            NodeMode::EspNowCentral => {
                Node::Central(CentralOpMode::EspNow(EspNowConfig::default()))
            }
            NodeMode::EspNowPeripheral => {
                Node::Peripheral(PeripheralOpMode::EspNow(EspNowConfig::default()))
            }
        };

        // Non-zero trigger_freq enables traffic generation
        let traffic_freq = if user_config.trigger_freq == 0 {
            None
        } else {
            Some(user_config.trigger_freq as u16)
        };

        // Build hardware handle and construct the CSI node
        let hardware = CSINodeHardware::new(&mut interfaces, controller);
        let mut node = CSINode::new(
            node_kind,
            user_config.collection_mode,
            Some(user_config.csi_config),
            traffic_freq,
            hardware,
        );
        // Protocol selection is mode-dependent:
        // - WifiStation uses P802D11BGNLR (BGN + LR) for compatibility with standard APs
        //   while still enabling the LR physical layer for improved CSI quality.
        // - Sniffer/ESP-NOW use pure P802D11LR for maximum range between ESP devices.
        match user_config.node_mode {
            NodeMode::WifiStation => {
                node.set_protocol(esp_radio::wifi::Protocol::P802D11BGNLR);
            }
            NodeMode::WifiSniffer | NodeMode::EspNowCentral | NodeMode::EspNowPeripheral => {
                node.set_protocol(esp_radio::wifi::Protocol::P802D11LR);
                node.set_rate(esp_radio::esp_now::WifiPhyRate::RateMcs0Lgi);
            }
        }

        // Run for a fixed duration or indefinitely.
        // run_duration handles printing internally via CSINodeClient.
        // run() alone only enqueues packets — join it with a print loop.
        match duration {
            Some(secs) => {
                let mut client = CSINodeClient::new();
                node.run_duration(secs, &mut client).await;
            }
            None => {
                let mut client = CSINodeClient::new();
                join(node.run(), async {
                    loop {
                        client.print_csi_w_metadata().await;
                    }
                })
                .await;
            }
        }

        // Unlock the CLI in the main loop
        DONE_SIGNAL.signal(());
    }
}
