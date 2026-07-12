#![no_std]
#![no_main]

mod cli;
mod config;

use crate::config::{
    DONE_SIGNAL, IS_COLLECTING, RESTART_PENDING, RESTART_SIGNAL, START_SIGNAL, STOP_REQUEST,
    USER_CONFIG, UserConfig,
};
// `is_jtag` only exists under runtime auto-detection (`auto`); forced
// `jtag-serial`/`uart` backends know their transport at compile time and never
// call it. Gate the import to match its definition so forced-backend builds
// (e.g. `defmt` + `jtag-serial`) compile.
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
#[cfg(all(
    feature = "auto",
    any(
        feature = "esp32c3",
        feature = "esp32c5",
        feature = "esp32c6",
        feature = "esp32s3"
    )
))]
use cli::is_jtag;
use cli::{Context, ROOT_MENU};
use core::sync::atomic::Ordering;
use embassy_executor::Spawner;
use embassy_futures::select::select;
// During collection the JTAG+auto build polls for 'q' with the raw OUT-EP peek
// only (2-arm `select`); other builds fall back to the async `Read` arm
// (`select3`). See the collection-stop loop in `main`.
// `Either` is the return type of the unconditional 2-arm `select` used by the
// restart-command loop in `csi_collection`, so it must be imported for every
// target. `Either3`/`select3` stay gated to the builds that use the 3-arm
// async `Read` arm.
use embassy_futures::select::Either;
#[cfg(not(all(
    any(feature = "esp32c3", feature = "esp32c5", feature = "esp32c6", feature = "esp32s3"),
    feature = "auto"
)))]
use embassy_futures::select::{Either3, select3};
use embassy_time::{Duration, Timer};
use embedded_io::Write;
use esp_backtrace as _;
use esp_bootloader_esp_idf::esp_app_desc;
use esp_csi_rs::logging::logging::{LogMode, init_logger};
use esp_csi_rs::{
    CSINode, CSINodeClient, CSINodeHardware, CentralOpMode, CollectionMode, CsiDeliveryMode,
    EspNowConfig, Node, PeripheralOpMode, WifiApConfig, WifiSnifferConfig, WifiStationConfig,
    clear_csi_callback, set_csi_delivery_mode, set_csi_logging_enabled, set_csi_raw_callback,
    set_raw_listen,
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
use esp_radio::wifi::ap::AccessPointConfig;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{AuthenticationMethod, Interfaces, PowerSaveMode, WifiController};
use menu::*;

esp_app_desc!();

extern crate alloc;
use alloc::string::ToString;

/// Reclaimed-RAM heap size per chip (link-tested against this firmware).
///
/// RISC-V parts (C3/C5/C6) top out at 64 KiB once the CLI + Wi-Fi stacks are
/// linked; C5 association needs more than the old 60 KiB default.
/// Xtensa parts have more `dram2` headroom — ESP32 can use the same 96 KiB
/// budget as esp-csi-rs sniffer experiments; ESP32-S3 fits 72 KiB.
#[cfg(feature = "esp32")]
const HEAP_SIZE: usize = 98_440;
#[cfg(feature = "esp32s3")]
const HEAP_SIZE: usize = 72_000;
#[cfg(any(feature = "esp32c3", feature = "esp32c5", feature = "esp32c6"))]
const HEAP_SIZE: usize = 65_536;

static WIFI_CONTROLLER: static_cell::StaticCell<WifiController<'static>> =
    static_cell::StaticCell::new();

/// Synchronously drain any pending byte from the USB-Serial-JTAG OUT FIFO via
/// raw register reads, returning `true` if a 'q'/'Q' is found in the burst.
///
/// Why bypass the async path: under heavy CSI traffic the WiFi/CSI ISR
/// pressure and `esp_println`'s critical sections starve the
/// `embedded_io_async::Read::read` waker on the JTAG peripheral, making the
/// CLI's interrupt-driven stop key probabilistic. The OUT-EP data-avail bit
/// in `EP1_CONF` and the FIFO read at `EP1` are exactly what esp-hal's own
/// `read_byte` polls — reading them here from the periodic CLI tick guarantees
/// the byte gets pulled out regardless of waker delivery.
///
/// Addresses match esp-println's `serial_jtag_printer` const block, which has
/// been verified for these chips.
#[cfg(all(
    any(
        feature = "esp32c3",
        feature = "esp32c5",
        feature = "esp32c6",
        feature = "esp32s3"
    ),
    feature = "auto"
))]
fn jtag_peek_for_stop() -> bool {
    #[cfg(feature = "esp32c3")]
    const EP1: *mut u32 = 0x6004_3000 as *mut u32;
    #[cfg(feature = "esp32c3")]
    const EP1_CONF: *const u32 = 0x6004_3004 as *const u32;

    #[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
    const EP1: *mut u32 = 0x6000_F000 as *mut u32;
    #[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
    const EP1_CONF: *const u32 = 0x6000_F004 as *const u32;

    #[cfg(feature = "esp32s3")]
    const EP1: *mut u32 = 0x6003_8000 as *mut u32;
    #[cfg(feature = "esp32s3")]
    const EP1_CONF: *const u32 = 0x6003_8004 as *const u32;

    // `serial_out_ep_data_avail` is bit 2 of EP1_CONF on these chips.
    const DATA_AVAIL: u32 = 0b100;

    let mut found = false;
    let mut guard = 64; // Bound the loop at the OUT-EP buffer size.
    while guard > 0 {
        guard -= 1;
        let conf = unsafe { EP1_CONF.read_volatile() };
        if conf & DATA_AVAIL == 0 {
            break;
        }
        let byte = unsafe { EP1.read_volatile() } as u8;
        if byte == b'q' || byte == b'Q' {
            found = true;
        }
    }
    found
}

/// Stub for build configurations that don't use the JTAG-auto path. The CLI
/// inner loop falls back to the async-read arm in those cases.
#[cfg(not(all(
    any(
        feature = "esp32c3",
        feature = "esp32c5",
        feature = "esp32c6",
        feature = "esp32s3"
    ),
    feature = "auto"
)))]
fn jtag_peek_for_stop() -> bool {
    false
}

/// Raw CSI fast-path callback (CPU-benchmark mode). The WiFi callback invokes
/// this and returns *before* building the ~640 B `CSIDataPacket`, so the
/// per-frame cost is just the dispatch — matching the ESP-IDF reference. No CSI
/// data is delivered or logged; stop relies on duration / reset / main-loop `q`.
fn raw_csi_noop() {}

/// Build the [`EspNowConfig`] for an ESP-NOW run from the user config snapshot.
///
/// `with_peer_mac` switches off automatic magic-prefix pairing for explicit
/// per-node peer filtering; `with_ht40` forces the per-peer TX PHY to HT40.
/// Both are only applied when the user configured them.
fn build_espnow_config(user_config: &UserConfig) -> EspNowConfig {
    let mut cfg = EspNowConfig::default()
        .with_channel(user_config.channel)
        .with_phy_rate(user_config.phy_rate);
    if let Some(mac) = user_config.peer_mac {
        cfg = cfg.with_peer_mac(mac);
    }
    if let Some(secondary) = user_config.ht40_secondary {
        cfg = cfg.with_ht40(secondary);
    }
    cfg
}

fn build_espnow_fast_config(user_config: &UserConfig) -> EspNowConfig {
    let mut cfg = EspNowConfig::fast_default().with_channel(user_config.channel);
    if let Some(mac) = user_config.peer_mac {
        cfg = cfg.with_peer_mac(mac);
    }
    if let Some(secondary) = user_config.ht40_secondary {
        cfg = cfg.with_ht40(secondary);
    }
    cfg
}

fn build_wifi_ap_config(user_config: &UserConfig) -> WifiApConfig {
    let auth = if user_config.ap_password.is_empty() {
        AuthenticationMethod::None
    } else {
        AuthenticationMethod::Wpa2Personal
    };
    let mut ap_radio_config = AccessPointConfig::default()
        .with_ssid(user_config.ap_ssid.as_str().to_string())
        .with_channel(user_config.channel)
        .with_auth_method(auth);
    if !user_config.ap_password.is_empty() {
        ap_radio_config =
            ap_radio_config.with_password(user_config.ap_password.as_str().to_string());
    }
    WifiApConfig::new(
        ap_radio_config,
        user_config.channel,
        user_config.ht40_secondary,
    )
    .with_dhcp_server(user_config.serve_dhcp)
    .with_lease_pool(user_config.ap_lease_count)
    .with_sync_burst(user_config.ap_sync_burst)
}

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
    /// Self-contained softAP CSI collector (DHCP + ICMP flood).
    WifiAccessPoint,
    /// Acts as the central (initiating) device in an ESP-NOW pair.
    EspNowCentral,
    /// Acts as the peripheral (responding) device in an ESP-NOW pair.
    EspNowPeripheral,
    /// Asymmetric ESP-NOW simplex collector (sparse beacon, then RX-only).
    EspNowFastCollector,
    /// Asymmetric ESP-NOW simplex source (unicast flood at forced PHY).
    EspNowFastSource,
}

/// esp-backtrace `custom-halt` hook: called after the panic message and
/// backtrace have been printed. Reboot into a clean CLI instead of parking
/// the CPU forever — a halted board answers nothing on serial (not even
/// `restart`), DTR/RTS resets don't reach the chip at runtime, and recovery
/// would otherwise require a physical button press, defeating webserver
/// automation. The spin-wait lets the USB-Serial-JTAG FIFO drain the panic
/// output before the reset wipes it.
#[unsafe(no_mangle)]
fn custom_halt() -> ! {
    for _ in 0..100_000_000u32 {
        core::hint::spin_loop();
    }
    rwdt_full_system_reset();
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // Initalize ESP device and acquire peripherals
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Initialize the CSI data logger before the heap so the logger task is
    // already in the executor by the time esp_rtos::start hands control over.
    init_logger(spawner, LogMode::ArrayList);

    // Allocate heap space in reclaimed RAM (sizes in [`HEAP_SIZE`] — per-chip max
    // that still links with the CLI + Wi-Fi stacks).
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: HEAP_SIZE);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    // Initialize ESP radio + Wi-Fi controller. v0.6.0 folded the standalone
    // `esp_radio::init()` call into `esp_radio::wifi::new`, so there is no
    // longer a separately-staticked radio controller.
    //
    // AMPDU aggregates multiple frames into one PPDU, which the CSI callback only
    // fires once for — fewer, clumpier CSI events and Block-Ack recovery stalls
    // after a lost subframe. Disabled globally for clean one-PPDU-per-frame CSI
    // capture, matching Espressif's esp-csi reference and esp-csi-rs's own
    // AP/STA examples. Halved dynamic buffer counts (32 → 16 each way)
    // bound the driver's transient draw on the shared esp-alloc heap, also
    // matching the examples; overflow surfaces as bounded netstack frame
    // drops, which CSI capture never sees (CSI comes from the PHY callback).
    let config_radio = esp_radio::wifi::ControllerConfig::default()
        .with_ampdu_rx_enable(false)
        .with_ampdu_tx_enable(false)
        .with_dynamic_rx_buf_num(16)
        .with_dynamic_tx_buf_num(16)
        .with_rx_ba_win(4);
    let (wifi_controller, interfaces) = esp_radio::wifi::new(peripherals.WIFI, config_radio)
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

    // Create a buffer to store CLI input. `menu` prints "Buffer overflow!" for
    // every byte once this is full (lib.rs:446), so we mirror its capacity in
    // the preprocessor below and stop forwarding before it fills — see
    // `forwarded`/`CLI_BUF_LEN`.
    const CLI_BUF_LEN: usize = 256;
    let mut clibuf = [0u8; CLI_BUF_LEN];
    // Instantiate Context placeholder
    let mut context = Context::default();

    let serial = {
        // ESP32: Always UART. esp-hal's `UartBuilder::new` ties the RX signal
        // to a constant logic-high until `with_rx` is called, so without
        // explicit GPIO3 assignment the UART never sees any incoming bytes
        // (TX works because the ROM bootloader already wired U0TXD → GPIO1).
        #[cfg(feature = "esp32")]
        {
            Uart::new(peripherals.UART0, esp_hal::uart::Config::default())
                .unwrap()
                .with_rx(peripherals.GPIO3)
                .with_tx(peripherals.GPIO1)
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
                SerialInterface::UsbJtag(UsbSerialJtag::new(peripherals.USB_DEVICE).into_async())
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
    let mut shadow: heapless::Vec<u8, CLI_BUF_LEN> = heapless::Vec::new();
    let mut quote_char: Option<u8> = None;
    // Mirrors `menu`'s internal `used` counter (bytes actually forwarded to
    // `input_byte`, i.e. printables + the 0x1F space sentinel, *not* swallowed
    // quote chars). When it reaches `CLI_BUF_LEN` we stop forwarding/echoing so
    // `menu` never hits its per-byte "Buffer overflow!" path — the user can keep
    // editing with backspace or submit with Enter, no manual flush required.
    let mut forwarded: usize = 0;
    // Latches the one-shot overflow bell so a held key / paste doesn't re-beep
    // on every dropped byte. Re-armed when space frees (backspace) or on submit.
    let mut at_capacity = false;

    loop {
        let mut buf = [0_u8; 1];

        if IS_COLLECTING.load(Ordering::Relaxed) {
            // Erase the spurious "> " the menu crate printed after the command returned
            Write::write_all(&mut runner.interface, b"\r\x1b[2K").ok();
            // CLI locked during collection: only 'q'/'Q' triggers an early stop.
            //
            // Latch so the `Stopping...` line prints once per run. Without
            // this, every subsequent 'q' the host pushes (or every key repeat
            // while the user holds it) re-fires the print before
            // `DONE_SIGNAL` arrives, producing a wall of duplicates.
            // Re-signalling `STOP_REQUEST` itself is harmless and idempotent.
            let mut stop_announced = false;

            // JTAG + `auto`: poll for 'q' with the raw OUT-EP peek ONLY. We must
            // NOT operate the CLI's async `UsbSerialJtag` reader during a run:
            // its `Read` toggles the shared USB_SERIAL_JTAG interrupt-enable
            // register, while esp-csi-rs's logger drives a *second*
            // `UsbSerialJtag` (output) on the same peripheral. Racing INT_ENA
            // updates between the two strand the logger's IN-empty wakeup, which
            // wedges CSI output mid-run — the executor stays alive (so 'q' via
            // this peek still stops it) and the peer keeps streaming, but this
            // node emits nothing until a stop/start. `jtag_peek_for_stop` reads
            // only FIFO data/status, never INT_ENA, so it can't cause that race.
            #[cfg(all(
                any(feature = "esp32c3", feature = "esp32c5", feature = "esp32c6", feature = "esp32s3"),
                feature = "auto"
            ))]
            loop {
                match select(DONE_SIGNAL.wait(), Timer::after(Duration::from_millis(5))).await {
                    Either::First(_) => break, // collection ended
                    Either::Second(_) => {
                        if jtag_peek_for_stop() {
                            STOP_REQUEST.signal(());
                            if !stop_announced {
                                Write::write_all(&mut runner.interface, b"\r\nStopping...\r\n")
                                    .ok();
                                stop_announced = true;
                            }
                        }
                    }
                }
            }

            // UART / non-peek builds: no raw peek exists, so the async `Read` is
            // the only way to see 'q'. A UART transport has no second output
            // driver on the same peripheral, so the INT_ENA race above does not
            // apply here. 32-byte chunk because USB-CDC delivers whole packets —
            // a single 'q' may share a packet with CR/LF.
            #[cfg(not(all(
                any(feature = "esp32c3", feature = "esp32c5", feature = "esp32c6", feature = "esp32s3"),
                feature = "auto"
            )))]
            {
                let mut chunk = [0_u8; 32];
                loop {
                    match select3(
                        embedded_io_async::Read::read(&mut runner.interface, &mut chunk),
                        DONE_SIGNAL.wait(),
                        Timer::after(Duration::from_millis(5)),
                    )
                    .await
                    {
                        Either3::First(res) => {
                            let n = res.unwrap_or(0);
                            if chunk[..n].iter().any(|&b| b == b'q' || b == b'Q') {
                                STOP_REQUEST.signal(());
                                if !stop_announced {
                                    Write::write_all(&mut runner.interface, b"\r\nStopping...\r\n")
                                        .ok();
                                    stop_announced = true;
                                }
                            }
                        }
                        Either3::Second(_) => break, // collection ended
                        Either3::Third(_) => {
                            if jtag_peek_for_stop() {
                                STOP_REQUEST.signal(());
                                if !stop_announced {
                                    Write::write_all(&mut runner.interface, b"\r\nStopping...\r\n")
                                        .ok();
                                    stop_announced = true;
                                }
                            }
                        }
                    }
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
                    if forwarded < CLI_BUF_LEN {
                        Write::write_all(&mut runner.interface, b" ").ok();
                        runner.input_byte(0x1F, &mut context);
                        let _ = shadow.push(0x1F);
                        forwarded += 1;
                    } else if !at_capacity {
                        // Input limit reached: ring the bell once, then drop
                        // further bytes silently (no echo, no forward) so menu's
                        // "Buffer overflow!" spam never fires.
                        Write::write_all(&mut runner.interface, b"\x07").ok();
                        at_capacity = true;
                    }
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
                        at_capacity = false;
                    }
                    Some(_) => {
                        runner.input_byte(b, &mut context);
                        quote_char = recompute_quote_state(&shadow);
                        forwarded = forwarded.saturating_sub(1);
                        at_capacity = false;
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
                        // `\r` is where menu resets its `used` counter, so reset
                        // our mirror too. (`\n` is stripped by menu without
                        // touching `used`, so leave the counter alone there.)
                        forwarded = 0;
                        at_capacity = false;
                        Write::write_all(&mut runner.interface, b"\r\x1b[2K").ok();
                    }
                    runner.input_byte(b, &mut context);
                }
                // Every other byte: echo, forward, push to shadow.
                _ => {
                    if forwarded < CLI_BUF_LEN {
                        Write::write_all(&mut runner.interface, &[b]).ok();
                        runner.input_byte(b, &mut context);
                        let _ = shadow.push(b);
                        forwarded += 1;
                    } else if !at_capacity {
                        // Input limit reached: ring the bell once, then drop
                        // further bytes silently so menu never spams
                        // "Buffer overflow!" and no manual flush is needed.
                        Write::write_all(&mut runner.interface, b"\x07").ok();
                        at_capacity = true;
                    }
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
        // Wait for a start signal from the CLI — or a restart request, which
        // this task must service because it owns the WiFi controller (the
        // radio must be deinitialized before the chip resets; see
        // `radio_off_and_restart`).
        let duration = match select(START_SIGNAL.wait(), RESTART_SIGNAL.wait()).await {
            Either::First(duration) => {
                START_SIGNAL.reset();
                duration
            }
            Either::Second(()) => radio_off_and_restart(controller).await,
        };

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
                let auth = if user_config.sta_password.is_empty() {
                    AuthenticationMethod::None
                } else {
                    AuthenticationMethod::Wpa2Personal
                };
                let mut client_config = StationConfig::default()
                    .with_ssid(user_config.sta_ssid.as_str().to_string())
                    .with_auth_method(auth);
                if !user_config.sta_password.is_empty() {
                    client_config = client_config
                        .with_password(user_config.sta_password.as_str().to_string());
                }
                Node::Central(CentralOpMode::WifiStation(
                    WifiStationConfig::new(client_config).with_channel_hint(user_config.channel),
                ))
            }
            NodeMode::WifiAccessPoint => Node::Central(CentralOpMode::WifiAccessPoint(
                build_wifi_ap_config(&user_config),
            )),
            NodeMode::EspNowCentral => {
                Node::Central(CentralOpMode::EspNow(build_espnow_config(&user_config)))
            }
            NodeMode::EspNowPeripheral => {
                Node::Peripheral(PeripheralOpMode::EspNow(build_espnow_config(&user_config)))
            }
            NodeMode::EspNowFastCollector => Node::Central(CentralOpMode::EspNowFastCollector(
                build_espnow_fast_config(&user_config),
            )),
            NodeMode::EspNowFastSource => Node::Peripheral(PeripheralOpMode::EspNowFastSource(
                build_espnow_fast_config(&user_config),
            )),
        };

        // Throughput-oriented modes disable Wi-Fi power saving (matches esp-csi-rs examples).
        if matches!(
            user_config.node_mode,
            NodeMode::WifiAccessPoint
                | NodeMode::WifiStation
                | NodeMode::EspNowFastCollector
                | NodeMode::EspNowFastSource
        ) {
            let _ = controller.set_power_saving(PowerSaveMode::None);
        }

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
        // `set-traffic --frequency-hz=0` must mean NO generated traffic: with
        // TX left on, esp-csi-rs falls back to a default flood rate when the
        // frequency is None (100 Hz station / 1000 Hz AP). Restricted to the
        // WiFi infra modes — ESP-NOW modes use the TX task for their own
        // transmissions, not the ICMP flood.
        let mut io_tasks = user_config.io_tasks;
        if user_config.trigger_freq == 0
            && matches!(
                user_config.node_mode,
                NodeMode::WifiAccessPoint | NodeMode::WifiStation
            )
        {
            io_tasks.tx_enabled = false;
        }
        node.set_io_tasks(io_tasks);
        // `set-traffic --unsolicited=on`: flood unsolicited echo replies —
        // one-directional traffic, no reply contention (see UserConfig doc).
        node.set_flood_unsolicited_reply(user_config.flood_unsolicited);

        // Apply the user-selected Wi-Fi PHY protocol (set via `set-protocol`).
        // ESP-NOW / sniffer modes additionally pin the PHY rate; station mode
        // derives its rate from the associated AP, so `set_rate` is a no-op there.
        node.set_protocol(user_config.protocol);
        if !matches!(user_config.node_mode, NodeMode::WifiStation) {
            node.set_rate(user_config.phy_rate);
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

        // CSI output: use esp-csi-rs's inline `log_csi` path (same as the
        // esp-csi-rs examples). Do **not** register a per-packet `set_csi_callback` here —
        // the old callback cloned every ~640 B frame and drained the JTAG RX FIFO
        // on each one, capping throughput around ~10 Hz. The main loop polls for
        // `q` every 5 ms while `IS_COLLECTING` (see below).
        clear_csi_callback();
        set_csi_delivery_mode(CsiDeliveryMode::Off);
        if user_config.delivery_raw {
            set_csi_logging_enabled(false);
            set_raw_listen(true);
            set_csi_raw_callback(raw_csi_noop);
        } else {
            set_raw_listen(false);
            match user_config.collection_mode {
                CollectionMode::Collector => set_csi_logging_enabled(true),
                CollectionMode::Listener => set_csi_logging_enabled(false),
            }
        }

        // Run for a fixed duration or indefinitely. In both arms run_duration/run
        // listens to esp-csi-rs's internal STOP_SIGNAL, so send_stop() unwinds them.
        match duration {
            Some(secs) => {
                let mut client = CSINodeClient::new();
                select(node.run_duration(secs, &mut client), stop_watcher).await;
            }
            None => {
                select(node.run(), stop_watcher).await;
            }
        }

        // Belt-and-braces: drop any pending stop request that arrived after the
        // collection already wound down on its own.
        STOP_REQUEST.reset();

        // Unlock the CLI in the main loop
        DONE_SIGNAL.signal(());

        // A `restart` issued mid-collection stops the run (via STOP_REQUEST)
        // and lands here: deinit the radio, then reset.
        if RESTART_PENDING.load(Ordering::Relaxed) {
            radio_off_and_restart(controller).await;
        }
    }
}

/// Deinit the radio, then hard-reset the chip via the RTC watchdog.
///
/// Resetting with the radio live has been observed to hang the next boot on
/// ESP32-C5: the ROM banner prints once and the application never starts —
/// only the EN button or a USB power cycle recovers. Running the controller's
/// destructor in place invokes esp-radio's `wifi_deinit` (full RF/driver
/// teardown) so the next boot starts from quiet hardware. The controller
/// reference is dead after `drop_in_place`, which is sound only because this
/// function diverges into the reset without touching it again.
async fn radio_off_and_restart(controller: &mut WifiController<'static>) -> ! {
    unsafe { core::ptr::drop_in_place(controller as *mut WifiController<'static>) };
    // Let the deinit settle and the "Restarting..." text drain the FIFO.
    Timer::after(Duration::from_millis(200)).await;
    rwdt_full_system_reset();
}

/// Deepest reset software can trigger: RWDT stage-0 `ResetSystem` resets the
/// main system, the power management unit AND the RTC peripherals. A plain
/// HP-system software reset (`rst:0x3 RTC_SW_HPSYS`) hangs the next boot on
/// these ESP32-C5 boards — the ROM banner prints and the application never
/// starts, radio deinit or not — while this watchdog reset is the closest
/// software equivalent of the EN button.
fn rwdt_full_system_reset() -> ! {
    use esp_hal::rtc_cntl::{Rtc, RwdtStage, RwdtStageAction};
    // Stealing LPWR is sound here: this function diverges into a chip reset,
    // so no other owner can observe the aliased peripheral afterwards.
    let mut rtc = Rtc::new(unsafe { esp_hal::peripherals::LPWR::steal() });
    rtc.rwdt.set_stage_action(RwdtStage::Stage0, RwdtStageAction::ResetSystem);
    rtc.rwdt
        .set_timeout(RwdtStage::Stage0, esp_hal::time::Duration::from_millis(50));
    rtc.rwdt.enable();
    loop {
        core::hint::spin_loop();
    }
}
