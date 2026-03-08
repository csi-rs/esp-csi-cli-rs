use core::cell::RefCell;
use core::sync::atomic::AtomicBool;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, blocking_mutex::Mutex, signal::Signal};
use esp_csi_rs::{CollectionMode, config::CsiConfig};
use heapless::String;

use crate::NodeMode;

pub static START_SIGNAL: Signal<CriticalSectionRawMutex, Option<u64>> = Signal::new();
/// Signals the main loop that CSI collection has ended; set by the collection task.
pub static DONE_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// True while CSI collection is active; the main loop locks the CLI when set.
pub static IS_COLLECTING: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
pub struct UserConfig {
    pub node_mode: NodeMode,
    pub collection_mode: CollectionMode,
    pub trigger_freq: u64,
    pub sta_ssid: heapless::String<32>,
    pub sta_password: heapless::String<32>,
    pub csi_config: CsiConfig,
    pub channel: u8,
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
            .finish()
    }
}

impl UserConfig {
    pub fn new() -> Self {
        UserConfig {
            node_mode: NodeMode::WifiSniffer,
            collection_mode: CollectionMode::Collector,
            trigger_freq: 100,
            sta_ssid: String::new(),
            sta_password: String::new(),
            csi_config: CsiConfig::default(),
            channel: 1,
        }
    }
}

pub static USER_CONFIG: Mutex<CriticalSectionRawMutex, RefCell<Option<UserConfig>>> =
    Mutex::new(RefCell::new(None));