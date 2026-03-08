use core::cell::RefCell;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, blocking_mutex::Mutex};
use esp_csi_rs::{CollectionMode, config::CsiConfig};
use heapless::String;

use crate::NodeMode;

#[derive(Debug, Clone)]
pub struct UserConfig {
    pub node_mode: NodeMode,
    pub collection_mode: CollectionMode,
    pub trigger_freq: u64,
    pub sta_ssid: heapless::String<32>,
    pub sta_password: heapless::String<32>,
    pub csi_config: CsiConfig,
    pub channel: u8,
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