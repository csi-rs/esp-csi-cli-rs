use core::cell::RefCell;
use core::sync::atomic::AtomicBool;

use embassy_sync::{
    blocking_mutex::Mutex, blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal,
};
use esp_csi_rs::{CollectionMode, IOTaskConfig, config::CsiConfig};
use esp_radio::esp_now::WifiPhyRate;
use esp_radio::wifi::{Protocol, SecondaryChannel};
use heapless::String;

use crate::NodeMode;

/// Default Wi-Fi channel for the build target. ESP32-C5 defaults to 5 GHz ch149;
/// all other chips default to 2.4 GHz ch1.
const fn default_wifi_channel() -> u8 {
    #[cfg(feature = "esp32c5")]
    {
        149
    }
    #[cfg(not(feature = "esp32c5"))]
    {
        1
    }
}

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

/// Set by the CLI `restart` command. The actual `software_reset` is performed
/// by the [`csi_collection`] task, which owns the WiFi controller: it deinits
/// the radio first (drops the controller → esp-radio `wifi_deinit`). Resetting
/// with the radio live has been observed to hang the next boot on ESP32-C5
/// (single ROM banner, application never starts, only the EN button recovers).
pub static RESTART_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// Companion flag to [`RESTART_SIGNAL`] checked after a collection run ends,
/// covering a `restart` issued mid-collection (the run is stopped first).
pub static RESTART_PENDING: AtomicBool = AtomicBool::new(false);

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
    /// ICMP flood sends unsolicited echo *replies* instead of echo requests.
    /// The peer's IP stack silently ignores unsolicited replies, so traffic is
    /// strictly one-directional: the peer still ACKs at the MAC level and
    /// captures CSI per frame, but never answers — halving on-air frames and
    /// stabilizing the offered rate. Trade-off: this node gets no CSI back
    /// from replies. Only meaningful for the WiFi AP/station flood.
    pub flood_unsolicited: bool,
    /// SSID used when operating in Station mode.
    pub sta_ssid: heapless::String<32>,
    /// Password used when operating in Station mode.
    pub sta_password: heapless::String<32>,
    /// SSID used when operating in softAP (wifi-ap) mode.
    pub ap_ssid: heapless::String<32>,
    /// Password used when operating in softAP mode. Empty = open network.
    pub ap_password: heapless::String<32>,
    /// Whether the built-in DHCP server runs in wifi-ap mode.
    pub serve_dhcp: bool,
    /// DHCP lease pool size in wifi-ap mode (1–8). With more than one lease
    /// the ICMP flood round-robins across all active leases, so every
    /// associated station receives traffic (and thus CSI). `1` restores the
    /// legacy single-target flood to the first lease address.
    pub ap_lease_count: u8,
    /// Synchronized burst flood in wifi-ap mode. When `true`, every flood tick
    /// sends one unicast frame back-to-back to *every* active lease, so all
    /// associated stations capture their downlink CSI within tens of
    /// microseconds of each other (time-aligned multi-receiver capture).
    /// When `false`, the flood round-robins one station per tick. Each
    /// receiver then sees the full `trigger_freq`, so total offered airtime is
    /// `trigger_freq × leases` — lower the rate if the channel saturates.
    pub ap_sync_burst: bool,
    /// Low-level CSI hardware configuration (feature flags, scale, etc.).
    pub csi_config: CsiConfig,
    /// WiFi channel to operate on (2.4 GHz: 1–14; 5 GHz on C5: 36–165). In
    /// station mode on ESP32-C5 this is also passed as the band-selection hint
    /// (`WifiStationConfig::channel_hint`) before association.
    pub channel: u8,
    /// Wi-Fi PHY rate. Only meaningful for ESP-NOW modes (sniffer/station
    /// derive their rate from the AP / radio configuration).
    pub phy_rate: WifiPhyRate,
    /// Wi-Fi PHY protocol applied to the node before a collection run
    /// (`CSINode::set_protocol`). Set via `set-protocol --protocol=<...>`.
    /// `LR` (Espressif long-range) is the default and suits sniffer / ESP-NOW
    /// links between ESP devices; use `N` when associating to a standard AP in
    /// station mode.
    pub protocol: Protocol,
    /// Per-direction task enables. Disabling RX turns the node into a
    /// pure transmitter (useful for asymmetric topologies); disabling
    /// TX turns it into a pure receiver (useful when the device is the
    /// passive end of an ESP-NOW pair).
    pub io_tasks: IOTaskConfig,
    /// Explicit ESP-NOW peer MAC. `Some(mac)` switches off automatic
    /// magic-prefix pairing in favor of an explicit per-node peer with
    /// source-MAC filtering (`EspNowConfig::with_peer_mac`). `None` keeps
    /// the default automatic pairing. ESP-NOW modes only.
    pub peer_mac: Option<[u8; 6]>,
    /// Forced HT40 transmit PHY for ESP-NOW. `Some(Above|Below)` forces the
    /// per-peer TX PHY to HT40 with the given secondary channel
    /// (`EspNowConfig::with_ht40`). `None` leaves the PHY at HT20/legacy per
    /// the selected rate. ESP-NOW modes only.
    pub ht40_secondary: Option<SecondaryChannel>,
    /// When `true`, the next collection run registers the zero-copy raw CSI
    /// fast-path (`set_csi_raw_callback`) instead of the full per-packet
    /// callback. Intended for CPU-cost benchmarking — no CSI data is delivered
    /// or logged in this mode. Set via `set-csi-delivery --mode=raw`.
    pub delivery_raw: bool,
}

impl core::fmt::Debug for UserConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let collection_mode_str = match self.collection_mode {
            CollectionMode::Collector => "Collector",
            CollectionMode::Listener => "Listener",
        };
        let ht40_str = match self.ht40_secondary {
            Some(SecondaryChannel::Above) => "Above",
            Some(SecondaryChannel::Below) => "Below",
            _ => "None",
        };
        f.debug_struct("UserConfig")
            .field("node_mode", &self.node_mode)
            .field("collection_mode", &collection_mode_str)
            .field("trigger_freq", &self.trigger_freq)
            .field("flood_unsolicited", &self.flood_unsolicited)
            .field("sta_ssid", &self.sta_ssid)
            .field("sta_password", &self.sta_password)
            .field("ap_ssid", &self.ap_ssid)
            .field("ap_password", &self.ap_password)
            .field("serve_dhcp", &self.serve_dhcp)
            .field("ap_lease_count", &self.ap_lease_count)
            .field("ap_sync_burst", &self.ap_sync_burst)
            .field("csi_config", &self.csi_config)
            .field("channel", &self.channel)
            .field("phy_rate", &self.phy_rate)
            .field("protocol", &self.protocol)
            .field("io_tasks", &self.io_tasks)
            .field("peer_mac", &self.peer_mac)
            .field("ht40_secondary", &ht40_str)
            .field("delivery_raw", &self.delivery_raw)
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
    /// | `flood_unsolicited` | `false` (echo requests) |
    /// | `sta_ssid`        | *(empty)*              |
    /// | `sta_password`    | *(empty)*              |
    /// | `ap_ssid`         | `esp-csi-ap`           |
    /// | `ap_password`     | *(empty)*              |
    /// | `serve_dhcp`      | `true`                 |
    /// | `ap_lease_count`  | `4`                    |
    /// | `ap_sync_burst`   | `false`                |
    /// | `csi_config`      | `CsiConfig::default()` |
    /// | `channel`         | `149` (C5) / `1` (others) |
    /// | `phy_rate`        | `WifiPhyRate::RateMcs0Lgi` |
    /// | `protocol`        | `Protocol::LR`         |
    /// | `io_tasks`        | TX + RX both enabled   |
    pub fn new() -> Self {
        UserConfig {
            node_mode: NodeMode::WifiSniffer,
            collection_mode: CollectionMode::Collector,
            trigger_freq: 100,
            flood_unsolicited: false,
            sta_ssid: String::new(),
            sta_password: String::new(),
            ap_ssid: {
                let mut s = String::new();
                let _ = s.push_str("esp-csi-ap");
                s
            },
            ap_password: String::new(),
            serve_dhcp: true,
            ap_lease_count: 4,
            ap_sync_burst: false,
            csi_config: CsiConfig::default(),
            channel: default_wifi_channel(),
            phy_rate: WifiPhyRate::RateMcs0Lgi,
            protocol: Protocol::LR,
            io_tasks: IOTaskConfig::default(),
            peer_mac: None,
            ht40_secondary: None,
            delivery_raw: false,
        }
    }
}

/// Global mutex-protected user configuration, accessible from both the CLI task and
/// the [`csi_collection`] task.
///
/// Initialised in `main` via [`UserConfig::new`] and mutated by CLI command handlers.
pub static USER_CONFIG: Mutex<CriticalSectionRawMutex, RefCell<Option<UserConfig>>> =
    Mutex::new(RefCell::new(None));
