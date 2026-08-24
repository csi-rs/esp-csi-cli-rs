use core::cell::RefCell;
use core::sync::atomic::AtomicBool;

use embassy_sync::{
    blocking_mutex::Mutex, blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal,
};
use esp_csi_rs::{IOTaskConfig, config::CsiConfig};
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
    /// WiFi/radio operating mode (sniffer, station, softAP, HT emitter, ESP-NOW central/peripheral).
    pub node_mode: NodeMode,
    /// Whether captured CSI is delivered off-device. When `false` the radio still
    /// captures — RX path and timing unchanged — but nothing is decoded or logged
    /// (`CSINode::set_csi_output_enabled`). Set via `set-csi-output --enabled=`.
    pub csi_output_enabled: bool,
    /// Restrict delivered CSI to this source MAC. `None` = accept every source.
    ///
    /// A collector is promiscuous: it reports CSI for the AP's beacons and ACKs and for any
    /// third-party device on the channel, not only for the traffic you configured. Set via
    /// `set-csi-filter --peer-mac=`.
    pub csi_peer_filter: Option<[u8; 6]>,
    /// Minimum `sig_mode` to deliver: `0` accepts every PHY, `1` keeps HT (802.11n) and better.
    ///
    /// `1` drops the legacy-rate mgmt/control frames that produce the short L-LTF-only rows at the
    /// head of a collection. Set via `set-csi-filter --min-phy=`.
    pub csi_min_sig_mode: u8,
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
    /// Wi-Fi PHY rate. Applied to the per-peer TX PHY of the ESP-NOW central /
    /// peripheral pair (`EspNowConfig::with_phy_rate`). Ignored elsewhere: the Wi-Fi
    /// collector modes derive their rate from the AP / radio configuration, an
    /// emitter transmits at the rate its forced TX PHY implies, and the fast simplex
    /// pair takes its rate from `EspNowConfig::fast_default()`.
    pub phy_rate: WifiPhyRate,
    /// Wi-Fi PHY protocol applied to the node before a collection run
    /// (`CSINode::set_protocol`). Set via `set-protocol --protocol=<...>`.
    /// `LR` (Espressif long-range) is the default and suits sniffer links
    /// between ESP devices; use `N` when associating to a standard AP in
    /// station mode.
    pub protocol: Protocol,
    /// Per-direction task enables. Disabling RX turns the node into a
    /// pure transmitter (useful for asymmetric topologies); disabling
    /// TX turns it into a pure receiver (no generated traffic).
    pub io_tasks: IOTaskConfig,
    /// Dual-purpose peer address, read differently per mode — one CLI field, two
    /// referents, because the underlying engine configs each have exactly one:
    ///
    /// - Emitter modes: destination of injected frames (`EmitterConfig::with_dst_mac`).
    ///   `None` = broadcast. Unicasting to a collector usually raises its CSI rate.
    /// - ESP-NOW modes: explicit peer (`EspNowConfig::with_peer_mac`). `None` keeps
    ///   automatic magic-prefix pairing; `Some` drops the magic prefix and filters on
    ///   source MAC instead, so BOTH nodes must be configured with the other's address.
    pub peer_mac: Option<[u8; 6]>,
    /// Secondary channel. For the softAP collector, `Some(Above|Below)` runs the AP
    /// as HT40 and `None` keeps it at HT20. For the ESP-NOW modes it forces the
    /// per-peer TX PHY to HT40 (`EspNowConfig::with_ht40`). It does **not** select
    /// emitter bandwidth — that is `--mode=ht40-emitter`.
    pub ht40_secondary: Option<SecondaryChannel>,
    /// When `true`, the next collection run registers the zero-copy raw CSI
    /// fast-path (`set_csi_raw_callback`) instead of the full per-packet
    /// callback. Intended for CPU-cost benchmarking — no CSI data is delivered
    /// or logged in this mode. Set via `set-csi-delivery --mode=raw`.
    pub delivery_raw: bool,
    /// Delay in milliseconds between injected frames for the emitter modes.
    /// Default `20` ms ≈ 50 frames/s. Set via `set-wifi --inject-period-ms=<ms>`.
    pub inject_period_ms: u32,
    /// Interface an emitter injects on: `true` = STA, `false` = AP. Raw injection
    /// is accepted on either, but which one actually radiates is chip-dependent,
    /// so this is exposed rather than hard-coded.
    pub emitter_use_sta_if: bool,
}

impl core::fmt::Debug for UserConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let ht40_str = match self.ht40_secondary {
            Some(SecondaryChannel::Above) => "Above",
            Some(SecondaryChannel::Below) => "Below",
            _ => "None",
        };
        f.debug_struct("UserConfig")
            .field("node_mode", &self.node_mode)
            .field("csi_output_enabled", &self.csi_output_enabled)
            .field("csi_peer_filter", &self.csi_peer_filter)
            .field("csi_min_sig_mode", &self.csi_min_sig_mode)
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
            .field("inject_period_ms", &self.inject_period_ms)
            .field("emitter_use_sta_if", &self.emitter_use_sta_if)
            .finish()
    }
}

impl UserConfig {
    /// Creates a [`UserConfig`] populated with sensible defaults:
    ///
    /// | Field             | Default                |
    /// |-------------------|------------------------|
    /// | `node_mode`       | `WifiSniffer`          |
    /// | `csi_output_enabled` | `true`              |
    /// | `csi_peer_filter` | `None` (any source)    |
    /// | `csi_min_sig_mode` | `0` (any PHY)         |
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
            csi_output_enabled: true,
            csi_peer_filter: None,
            csi_min_sig_mode: 0,
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
            inject_period_ms: 20,
            emitter_use_sta_if: true,
        }
    }
}

/// Global mutex-protected user configuration, accessible from both the CLI task and
/// the [`csi_collection`] task.
///
/// Initialised in `main` via [`UserConfig::new`] and mutated by CLI command handlers.
pub static USER_CONFIG: Mutex<CriticalSectionRawMutex, RefCell<Option<UserConfig>>> =
    Mutex::new(RefCell::new(None));
