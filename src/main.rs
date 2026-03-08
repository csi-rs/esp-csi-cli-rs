#![no_std]
#![no_main]

mod cli;
mod config;

use alloc::string::ToString;
use cli::{is_jtag, Context, SerialInterface, ROOT_MENU};
use esp_csi_rs::CentralOpMode;
use esp_csi_rs::WifiStationConfig;
use core::cell::RefCell;
use core::fmt::Write;
use core::u64;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::with_timeout;
use embassy_time::Duration;
use embassy_time::Timer;
use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_csi_rs::config::CsiConfig;
use esp_csi_rs::CollectionMode;
use esp_csi_rs::Node;
use esp_csi_rs::PeripheralOpMode;
use esp_csi_rs::WifiSnifferConfig;
use esp_hal::peripherals::Peripherals;
use esp_hal::timer::timg::TimerGroup;
#[cfg(any(feature = "esp32", feature = "auto"))]
use esp_hal::uart::{Config, Uart};
#[cfg(not(feature = "esp32"))]
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::Async;
use esp_println::print;
use esp_println::println;
use esp_radio::wifi::Interfaces;
use esp_radio::wifi::WifiController;
use heapless::String;
use menu::*;

use crate::config::UserConfig;

esp_app_desc!();

extern crate alloc;

static WIFI_CONTROLLER: static_cell::StaticCell<WifiController<'static>> =
    static_cell::StaticCell::new();

// static CSI_COLLECTOR: Mutex<CriticalSectionRawMutex, RefCell<Option<CSICollector>>> =
//     Mutex::new(RefCell::new(None));

#[derive(Debug, Clone)]
enum NodeMode {
    WifiSniffer,
    WifiStation,
    EspNowCentral,
    EspNowPeripheral,
}

static START_SIGNAL: Signal<CriticalSectionRawMutex, Option<u64>> = Signal::new();

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
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
    #[cfg(any(feature = "esp32c6", feature = "esp32c3"))]
    {
        let sw_interrupt =
            esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
        esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);
    }
    #[cfg(not(any(feature = "esp32c6", feature = "esp32c3")))]
    esp_rtos::start(timg0.timer0);

    // Initialize ESP radio Controller
    let radio_init = mk_static!(
        esp_radio::Controller<'static>,
        esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller")
    );

    let mut config_radio = esp_radio::wifi::Config::default();
    config_radio = config_radio.with_power_save_mode(esp_radio::wifi::PowerSaveMode::None);
    let (wifi_controller, mut interfaces) =
        esp_radio::wifi::new(radio_init, peripherals.WIFI, config_radio)
            .expect("Failed to initialize Wi-Fi controller");

    let controller = WIFI_CONTROLLER.init(wifi_controller);

    // Create an instance for User Configurations
    let user_config = UserConfig::new();

    // Pass User Config Instance to Global Context
    USER_CONFIG.lock(|config| {
        config.replace(Some(user_config));
    });

    // Spawn the CSI Collection Task (TODO)

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
                .with_tx(peripherals.GPIO1)
                .with_rx(peripherals.GPIO3)
                .into_async()
        }

        // Forced JTAG
        #[cfg(all(not(feature = "esp32"), not(feature = "auto"), feature = "jtag-serial"))]
        {
            UsbSerialJtag::new(peripherals.USB_DEVICE).into_async()
        }

        // Forced UART
        #[cfg(all(
            not(feature = "esp32"),
            not(feature = "auto"),
            not(feature = "jtag-serial")
        ))]
        {
            Uart::new(peripherals.UART0, esp_hal::uart::Config::default())
                .unwrap()
                .with_tx(peripherals.GPIO1) // Adjust pins as needed
                .with_rx(peripherals.GPIO3)
                .into_async()
        }

        // Runtime Auto-Detection
        #[cfg(all(not(feature = "esp32"), feature = "auto"))]
        {
            if is_jtag() {
                SerialInterface::UsbJtag(UsbSerialJtag::new(peripherals.USB_DEVICE).into_async())
            } else {
                SerialInterface::Uart(
                    Uart::new(peripherals.UART0, esp_hal::uart::Config::default())
                        .unwrap()
                        .with_tx(peripherals.GPIO1) // Adjust pins as needed
                        .with_rx(peripherals.GPIO3)
                        .into_async(),
                )
            }
        }
    };

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