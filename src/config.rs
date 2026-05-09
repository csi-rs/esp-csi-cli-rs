use core::cell::RefCell;
use core::sync::atomic::AtomicBool;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, blocking_mutex::Mutex, signal::Signal};
use esp_csi_rs::{CollectionMode, IOTaskConfig, config::CsiConfig};
use esp_radio::esp_now::WifiPhyRate;
use heapless::String;

use crate::NodeMode;

/// Sent by the CLI `start` command to the [`csi_collection`] task.
///
/// `Some(secs)` requests a timed run; `None` runs indefinitely.
pub static START_SIGNAL: Signal<CriticalSectionRawMutex, Option<u64>> = Signal::new();
/// Signals the main loop that CSI collection has ended; set by the collection task.
pub static DONE_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// Set by the main loop when the user presses the stop key during collection.
/// Observed by the `csi_collection` task, which then calls `CSINodeClient::send_stop()`
/// to unwind `run`/`run_duration` through esp-csi-rs's internal stop signal.
pub static STOP_REQUEST: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// True while CSI collection is active; the main loop locks the CLI when set.
pub static IS_COLLECTING: AtomicBool = AtomicBool::new(false);

/// Runtime configuration for the CSI node, edited live through the CLI.
///
/// An instance is stored in [`USER_CONFIG`] and snapshotted by the
/// [`csi_collection`] task at the start of each collection run.
#[derive(Clone)]
pub struct UserConfig {
    /// WiFi/radio operating mode (sniffer, station, ESP-NOW central/peripheral).
    pub node_mode: NodeMode,
    /// Whether the node actively collects (`Collector`) or passively receives (`Listener`).
    pub collection_mode: CollectionMode,
    /// Traffic generation frequency in Hz. `0` disables traffic generation.
    pub trigger_freq: u64,
    /// SSID used when operating in Station mode.
    pub sta_ssid: heapless::String<32>,
    /// Password used when operating in Station mode.
    pub sta_password: heapless::String<32>,
    /// Low-level CSI hardware configuration (feature flags, scale, etc.).
    pub csi_config: CsiConfig,
    /// WiFi channel to operate on (1–14).
    pub channel: u8,
    /// Wi-Fi PHY rate. Only meaningful for ESP-NOW modes (sniffer/station
    /// derive their rate from the AP / radio configuration).
    pub phy_rate: WifiPhyRate,
    /// Per-direction task enables. Disabling RX turns the node into a
    /// pure transmitter (useful for asymmetric topologies); disabling
    /// TX turns it into a pure receiver (useful when the device is the
    /// passive end of an ESP-NOW pair).
    pub io_tasks: IOTaskConfig,
}

impl core::fmt::Debug for UserConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let collection_mode_str = match self.collection_mode {
            CollectionMode::Collector => "Collector",
            CollectionMode::Listener => "Listener",
        };
        f.debug_struct("UserConfig")
            .field("node_mode", &self.node_mode)
            .field("collection_mode", &collection_mode_str)
            .field("trigger_freq", &self.trigger_freq)
            .field("sta_ssid", &self.sta_ssid)
            .field("sta_password", &self.sta_password)
            .field("csi_config", &self.csi_config)
            .field("channel", &self.channel)
            .field("phy_rate", &self.phy_rate)
            .field("io_tasks", &self.io_tasks)
            .finish()
    }
}

impl UserConfig {
    /// Creates a [`UserConfig`] populated with sensible defaults:
    ///
    /// | Field             | Default                |
    /// |-------------------|------------------------|
    /// | `node_mode`       | `WifiSniffer`          |
    /// | `collection_mode` | `Collector`            |
    /// | `trigger_freq`    | `100` Hz               |
    /// | `sta_ssid`        | *(empty)*              |
    /// | `sta_password`    | *(empty)*              |
    /// | `csi_config`      | `CsiConfig::default()` |
    /// | `channel`         | `1`                    |
    /// | `phy_rate`        | `WifiPhyRate::RateMcs0Lgi` |
    /// | `io_tasks`        | TX + RX both enabled   |
    pub fn new() -> Self {
        UserConfig {
            node_mode: NodeMode::WifiSniffer,
            collection_mode: CollectionMode::Collector,
            trigger_freq: 100,
            sta_ssid: String::new(),
            sta_password: String::new(),
            csi_config: CsiConfig::default(),
            channel: 1,
            phy_rate: WifiPhyRate::RateMcs0Lgi,
            io_tasks: IOTaskConfig::default(),
        }
    }
}

/// Global mutex-protected user configuration, accessible from both the CLI task and
/// the [`csi_collection`] task.
///
/// Initialised in `main` via [`UserConfig::new`] and mutated by CLI command handlers.
pub static USER_CONFIG: Mutex<CriticalSectionRawMutex, RefCell<Option<UserConfig>>> =
    Mutex::new(RefCell::new(None));