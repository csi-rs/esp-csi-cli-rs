#![no_std]
#![no_main]

mod cli;
mod config;

use crate::config::{UserConfig, DONE_SIGNAL, IS_COLLECTING, START_SIGNAL, STOP_REQUEST, USER_CONFIG};
#[cfg(any(
    feature = "esp32c3",
    feature = "esp32c5",
    feature = "esp32c6",
    feature = "esp32s3"
))]
use cli::is_jtag;
use cli::{Context, ROOT_MENU};
#[cfg(all(
    feature = "auto",
    any(
        feature = "esp32c3",
        feature = "esp32c5",
        feature = "esp32c6",
        feature = "esp32s3"
    )
))]
use cli::SerialInterface;
use core::sync::atomic::Ordering;
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embedded_io::Write;
use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_csi_rs::logging::logging::{init_logger, LogMode};
use esp_csi_rs::{
    CSINode, CSINodeClient, CSINodeHardware, CentralOpMode, EspNowConfig, Node, PeripheralOpMode,
    WifiSnifferConfig, WifiStationConfig,
};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
#[cfg(any(feature = "auto", feature = "uart"))]
use esp_hal::uart::Uart;
#[cfg(all(
    any(feature = "auto", feature = "jtag-serial"),
    any(
        feature = "esp32c3",
        feature = "esp32c5",
        feature = "esp32c6",
        feature = "esp32s3"
    )
))]
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{AuthenticationMethod, Interfaces, WifiController};
use menu::*;

esp_app_desc!();

extern crate alloc;
use alloc::string::ToString;

static WIFI_CONTROLLER: static_cell::StaticCell<WifiController<'static>> =
    static_cell::StaticCell::new();

/// Walk the per-line shadow buffer and return the open quote char, if any.
///
/// Counts `'` / `"` in order: an unmatched opening quote leaves us "inside"
/// that quote style; matching pairs cancel out; a quote of the wrong style
/// while already inside another quote is treated as a literal (matches the
/// `match` arms in the input loop). Called after each backspace to keep
/// `quote_char` consistent with the visible line.
fn recompute_quote_state(shadow: &[u8]) -> Option<u8> {
    let mut state: Option<u8> = None;
    for &b in shadow {
        match (b, state) {
            (b'"' | b'\'', None) => state = Some(b),
            (c, Some(q)) if c == q => state = None,
            _ => {}
        }
    }
    state
}

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

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // Initalize ESP device and acquire peripherals
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Initialize the CSI data logger before the heap so the logger task is
    // already in the executor by the time esp_rtos::start hands control over.
    init_logger(spawner, LogMode::ArrayList);

    // Allocate heap space. v0.6.0 places the allocator in reclaimed RAM so
    // internal RAM stays available for Wi-Fi / RTOS task stacks.
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 61440);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    // Initialize ESP radio + Wi-Fi controller. v0.6.0 folded the standalone
    // `esp_radio::init()` call into `esp_radio::wifi::new`, so there is no
    // longer a separately-staticked radio controller.
    let config_radio = esp_radio::wifi::ControllerConfig::default();
    let (wifi_controller, interfaces) =
        esp_radio::wifi::new(peripherals.WIFI, config_radio)
            .expect("Failed to initialize Wi-Fi controller");

    let controller = WIFI_CONTROLLER.init(wifi_controller);

    // Create an instance for User Configurations
    let user_config = UserConfig::new();

    // Pass User Config Instance to Global Context
    USER_CONFIG.lock(|config| {
        config.replace(Some(user_config));
    });

    // Spawn the CSI Collection Task. embassy-executor 0.10 changed the task
    // macro to return `Result<SpawnToken, SpawnError>` (so that runtime arity
    // mismatches surface as errors); unwrap before handing the token to the
    // spawner, which itself now returns `()` instead of a `Result`.
    spawner.spawn(
        csi_collection(interfaces, controller).expect("failed to spawn csi_collection task"),
    );

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
            any(
                feature = "esp32c3",
                feature = "esp32c5",
                feature = "esp32c6",
                feature = "esp32s3"
            )
        ))]
        {
            UsbSerialJtag::new(peripherals.USB_DEVICE).into_async()
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
            any(
                feature = "esp32c3",
                feature = "esp32c5",
                feature = "esp32c6",
                feature = "esp32s3"
            ),
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
    };

    // Instantiate CLI runner with root menu, buffer, and serial
    let mut runner = Runner::new(ROOT_MENU, &mut clibuf, serial, &mut context);

    // Byte-stream preprocessor state. `menu`'s per-keystroke echo is disabled
    // (see Cargo.toml comment) so we echo every visible char ourselves, which
    // lets quote chars and in-quote spaces render correctly.
    //
    // `shadow` mirrors every visible character on the current input line in
    // typed order. Quote chars are swallowed (echoed but NOT forwarded to
    // menu's input_byte), so menu's internal `used` counter can't erase them
    // on backspace — we have to do that ourselves. Walking the shadow on each
    // backspace also lets us recompute `quote_char` correctly when the popped
    // char is a quote.
    //
    // `quote_char` records which delimiter opened the current quote, so only
    // the matching one closes it (allowing the other quote style to appear
    // literally inside the value). Both `'` and `"` are accepted because some
    // serial terminals / keyboard layouts make Shift-' awkward to type.
    let mut shadow: heapless::Vec<u8, 256> = heapless::Vec::new();
    let mut quote_char: Option<u8> = None;

    loop {
        let mut buf = [0_u8; 1];

        if IS_COLLECTING.load(Ordering::Relaxed) {
            // Erase the spurious "> " the menu crate printed after the command returned
            Write::write_all(&mut runner.interface, b"\r\x1b[2K").ok();
            // CLI locked during collection: only 'q'/'Q' triggers an early stop;
            // every other byte is discarded until DONE_SIGNAL fires.
            loop {
                match select(
                    embedded_io_async::Read::read(&mut runner.interface, &mut buf),
                    DONE_SIGNAL.wait(),
                )
                .await
                {
                    Either::First(_) => {
                        if buf[0] == b'q' || buf[0] == b'Q' {
                            STOP_REQUEST.signal(());
                            Write::write_all(&mut runner.interface, b"\r\nStopping...\r\n").ok();
                        }
                    }
                    Either::Second(_) => break, // collection ended
                }
            }
            IS_COLLECTING.store(false, Ordering::Relaxed);
            // Reset preprocessor state: a half-open quote / stale shadow
            // shouldn't leak into the next CLI prompt.
            quote_char = None;
            shadow.clear();
            // \r       — move to start of line (overwrites the spurious "> " the menu crate printed)
            // \x1b[2K  — ANSI: erase the entire current line
            Write::write_all(
                &mut runner.interface,
                b"\r\x1b[2KCollection complete.\r\n> ",
            )
            .ok();
        } else {
            // Normal CLI mode. `menu`'s `echo` feature is disabled in Cargo.toml;
            // we echo every byte ourselves so the user sees what they typed
            // *as typed* rather than the post-substitution buffer (which would
            // hide the 0x1F sentinel and the swallowed quote chars).
            embedded_io_async::Read::read(&mut runner.interface, &mut buf)
                .await
                .unwrap();
            let b = buf[0];
            match (b, quote_char) {
                // Opening quote: echo and remember in shadow; do NOT forward.
                (b'"' | b'\'', None) => {
                    quote_char = Some(b);
                    Write::write_all(&mut runner.interface, &[b]).ok();
                    let _ = shadow.push(b);
                }
                // Matching closing quote: echo and remember; do NOT forward.
                (c, Some(q)) if c == q => {
                    quote_char = None;
                    Write::write_all(&mut runner.interface, &[b]).ok();
                    let _ = shadow.push(b);
                }
                // Space inside quotes: echo a real space for visibility, but
                // forward 0x1F (US) so menu's whitespace tokenizer doesn't
                // split. The command handler decodes 0x1F → ' ' on read-back.
                (b' ', Some(_)) => {
                    Write::write_all(&mut runner.interface, b" ").ok();
                    runner.input_byte(0x1F, &mut context);
                    let _ = shadow.push(0x1F);
                }
                // Backspace / DEL: pop the shadow and erase the corresponding
                // char. Swallowed quote chars aren't in menu's buffer, so we
                // have to write `\b \b` ourselves; everything else gets routed
                // through menu's backspace handler (which writes `\b \b` and
                // decrements its internal buffer).
                (0x08 | 0x7F, _) => match shadow.pop() {
                    Some(b'"') | Some(b'\'') => {
                        Write::write_all(&mut runner.interface, b"\x08 \x08").ok();
                        quote_char = recompute_quote_state(&shadow);
                    }
                    Some(_) => {
                        runner.input_byte(b, &mut context);
                        quote_char = recompute_quote_state(&shadow);
                    }
                    None => {} // nothing to erase
                },
                // Enter: drop any half-open quote (so it can't leak into the
                // next line), reset shadow, then forward to menu so it can
                // process the command. menu emits its own newline + command
                // echo + prompt afterwards (its `not(echo)` branch handles it).
                //
                // On `\r` we erase the input line first: menu 0.6.1 in
                // `not(feature = "echo")` mode unconditionally writes `\r`
                // followed by the buffered command (lib.rs:401-406). Without
                // erasing first, that overwrites only the leftmost N chars of
                // our already-echoed `> command`, leaving the trailing chars
                // of the prompt visible (e.g. "> info" → "infofo"). `\n` is
                // stripped by menu's input_byte and must not trigger the
                // erase, otherwise the prompt menu just printed gets wiped.
                (b'\r' | b'\n', _) => {
                    quote_char = None;
                    shadow.clear();
                    if b == b'\r' {
                        Write::write_all(&mut runner.interface, b"\r\x1b[2K").ok();
                    }
                    runner.input_byte(b, &mut context);
                }
                // Every other byte: echo, forward, push to shadow.
                _ => {
                    Write::write_all(&mut runner.interface, &[b]).ok();
                    runner.input_byte(b, &mut context);
                    let _ = shadow.push(b);
                }
            }
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

        // Map NodeMode → esp-csi-rs Node + operation mode. The configured
        // channel and PHY rate flow through the per-mode builders so a user
        // who sets `set-wifi --set-channel=6` then `start`s gets channel 6
        // applied even though set_channel is not called on the running node.
        let node_kind = match user_config.node_mode {
            NodeMode::WifiSniffer => Node::Peripheral(PeripheralOpMode::WifiSniffer(
                WifiSnifferConfig::default().with_channel(user_config.channel),
            )),
            NodeMode::WifiStation => {
                let client_config = StationConfig::default()
                    .with_ssid(user_config.sta_ssid.as_str().to_string())
                    .with_password(user_config.sta_password.as_str().to_string())
                    .with_auth_method(AuthenticationMethod::Wpa2Personal);
                Node::Central(CentralOpMode::WifiStation(WifiStationConfig {
                    client_config,
                }))
            }
            NodeMode::EspNowCentral => Node::Central(CentralOpMode::EspNow(
                EspNowConfig::default()
                    .with_channel(user_config.channel)
                    .with_phy_rate(user_config.phy_rate),
            )),
            NodeMode::EspNowPeripheral => Node::Peripheral(PeripheralOpMode::EspNow(
                EspNowConfig::default()
                    .with_channel(user_config.channel)
                    .with_phy_rate(user_config.phy_rate),
            )),
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
        // Apply IO task configuration (TX/RX direction toggles).
        node.set_io_tasks(user_config.io_tasks);

        // Protocol selection is mode-dependent:
        // - WifiStation uses 802.11n (or AX on Wi-Fi 6 capable parts) for
        //   compatibility with standard APs.
        // - Sniffer/ESP-NOW use the LR physical layer for maximum range
        //   between ESP devices, paired with the user-selected PHY rate.
        match user_config.node_mode {
            NodeMode::WifiStation => {
                #[cfg(feature = "esp32c6")]
                node.set_protocol(esp_radio::wifi::Protocol::AX);
                #[cfg(not(feature = "esp32c6"))]
                node.set_protocol(esp_radio::wifi::Protocol::N);
            }
            NodeMode::WifiSniffer | NodeMode::EspNowCentral | NodeMode::EspNowPeripheral => {
                node.set_protocol(esp_radio::wifi::Protocol::LR);
                node.set_rate(user_config.phy_rate);
            }
        }

        // Watcher that translates a CLI STOP_REQUEST into esp-csi-rs's internal
        // STOP_SIGNAL via the public CSINodeClient::send_stop API. After signaling
        // it parks forever so `select` resolves on the collection arm, letting
        // esp-csi-rs's normal teardown path (reset_globals, etc.) run.
        let stop_watcher = async {
            STOP_REQUEST.wait().await;
            STOP_REQUEST.reset();
            let stopper = CSINodeClient::new();
            stopper.send_stop().await;
            core::future::pending::<()>().await
        };

        // Run for a fixed duration or indefinitely. In both arms run_duration/run
        // listens to esp-csi-rs's internal STOP_SIGNAL, so send_stop() unwinds them.
        match duration {
            Some(secs) => {
                let mut client = CSINodeClient::new();
                select(node.run_duration(secs, &mut client), stop_watcher).await;
            }
            None => {
                let mut client = CSINodeClient::new();
                let collection = async {
                    select(
                        node.run(),
                        async {
                            loop {
                                client.print_csi_w_metadata().await;
                            }
                        },
                    )
                    .await;
                };
                select(collection, stop_watcher).await;
            }
        }

        // Belt-and-braces: drop any pending stop request that arrived after the
        // collection already wound down on its own.
        STOP_REQUEST.reset();

        // Unlock the CLI in the main loop
        DONE_SIGNAL.signal(());
    }
}
