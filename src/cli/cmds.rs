use core::cell::RefCell;
use core::sync::atomic::Ordering;
use embedded_io::Write;
use esp_csi_rs::logging::logging::set_log_mode as csi_set_log_mode;
use esp_csi_rs::logging::logging::LogMode;
use esp_csi_rs::{set_csi_delivery_mode, set_csi_logging_enabled, CsiDeliveryMode};
#[cfg(feature = "statistics")]
use esp_csi_rs::{
    get_dropped_packets_rx, get_pps_rx, get_pps_tx, get_rx_rate_hz, get_total_rx_packets,
    get_total_tx_packets, get_tx_rate_hz,
};
#[cfg(feature = "statistics")]
use esp_csi_rs::central::esp_now::{
    get_tx_confirmed_packets, get_tx_failed_packets, get_tx_queued_packets,
};
use esp_radio::esp_now::WifiPhyRate;
use esp_radio::wifi::{Protocol, SecondaryChannel};

use menu::{Item, Menu, argument_finder};

use crate::{NodeMode, cli::{Context, SerialInterface}, config::{IS_COLLECTING, START_SIGNAL, USER_CONFIG, UserConfig}};

/// Wire-format version of the firmware identification block emitted by
/// [`cli_info`] and the welcome banner. Bump on any breaking change to the
/// `info` grammar so host-side tooling can refuse incompatible firmware.
///
/// v2: added the `mac=` line to `info` (and the welcome banner). Host tooling
/// pins per-device tasks to this stable serial number rather than the
/// `/dev/ttyACM*` path, so a `restart` and the USB re-enumeration it triggers
/// re-bind to the same physical board whatever device node it returns as.
pub const CLI_PROTOCOL_VERSION: u32 = 2;

/// Read the factory base MAC address from eFuse.
///
/// This is the stable per-board identifier that the USB-Serial-JTAG stack also
/// surfaces as the USB `iSerialNumber` descriptor on native-USB boards (e.g.
/// `D0:CF:13:E2:90:E8`). Host tooling keys device identity off this value
/// instead of the enumeration-order `/dev/ttyACM*` path, which makes a
/// `restart` (and the re-enumeration it causes) a non-event: the per-device
/// task re-binds to the same board no matter which node number it comes back as.
pub fn device_mac() -> [u8; 6] {
    let mut out = [0u8; 6];
    out.copy_from_slice(esp_hal::efuse::base_mac_address().as_bytes());
    out
}

/// Parse a MAC address of the form `aa:bb:cc:dd:ee:ff` (also accepting `-`
/// separators), case-insensitive. Returns `None` on any malformed input.
fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let mut bytes = [0u8; 6];
    let mut count = 0;
    for part in s.split([':', '-']) {
        if count >= 6 || part.len() != 2 {
            return None;
        }
        bytes[count] = u8::from_str_radix(part, 16).ok()?;
        count += 1;
    }
    if count == 6 { Some(bytes) } else { None }
}

/// CLI command: `set-traffic`
///
/// Configures the CSI traffic generation frequency stored in [`USER_CONFIG`].
///
/// # Options
/// - `--frequency-hz=<NUMBER>` — Traffic generation rate in Hz. `0` disables traffic.
///
/// Prints the updated frequency after applying the change.
pub fn set_traffic<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let traffic_interval = argument_finder(item, args, "frequency-hz");

    match traffic_interval {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap().parse::<u64>() {
                    Ok(interval) => USER_CONFIG.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().trigger_freq = interval
                    }),
                    Err(_) => writeln!(serial, "Invalid Frequency").unwrap(),
                }
            }
        }
        Err(_) => (),
    }

    writeln!(serial, "\nUpdated Traffic Configuration:\n").unwrap();
    USER_CONFIG.lock(|config| {
        writeln!(
            serial,
            "Traffic Frequency: {}Hz",
            config.borrow().as_ref().unwrap().trigger_freq
        )
        .unwrap();
    });
}

#[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
/// CLI command: `set-csi` (ESP32-C6 variant)
///
/// Configures ESP32-C6-specific HE/STBC CSI acquisition flags in [`USER_CONFIG`].
///
/// # Options
/// - `--disable-csi`             — Disable CSI acquisition entirely.
/// - `--disable-csi-legacy`      — Disable L-LTF for 11g PPDUs.
/// - `--disable-csi-ht20`        — Disable HT-LTF for HT20 PPDUs.
/// - `--disable-csi-ht40`        — Disable HT-LTF for HT40 PPDUs.
/// - `--disable-csi-su`          — Disable HE-LTF for HE20 SU PPDUs.
/// - `--disable-csi-mu`          — Disable HE-LTF for HE20 MU PPDUs.
/// - `--disable-csi-dcm`         — Disable HE-LTF for HE20 DCM PPDUs.
/// - `--disable-csi-beamformed`  — Disable HE-LTF for HE20 Beamformed PPDUs.
/// - `--csi-he-stbc=<0-2>`       — STBC HE LTF selection (default: 2).
/// - `--val-scale-cfg=<0-3>`     — Value scale configuration (default: 2).
///
/// Prints the updated CSI configuration after applying changes.
pub fn set_csi<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
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
                USER_CONFIG
                    .lock(|config| config.borrow_mut().as_mut().unwrap().csi_config.enable = 0)
            }
        }
        Err(_) => (),
    }
    match disable_csi_legacy {
        Ok(str) => {
            if str.is_some() {
                USER_CONFIG.lock(|config| {
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
                USER_CONFIG.lock(|config| {
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
                USER_CONFIG.lock(|config| {
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
                USER_CONFIG.lock(|config| {
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
                USER_CONFIG.lock(|config| {
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
                USER_CONFIG.lock(|config| {
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
                USER_CONFIG.lock(|config| {
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
                    Ok(val) => USER_CONFIG.lock(|config| {
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
                    Ok(val) => USER_CONFIG.lock(|config| {
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
    USER_CONFIG.lock(|config| {
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

#[cfg(not(any(feature = "esp32c5", feature = "esp32c6")))]
/// CLI command: `set-csi` (non-ESP32-C6 variant)
///
/// Configures classic CSI acquisition feature flags in [`USER_CONFIG`].
///
/// # Options
/// - `--disable-lltf`        — Disable LLTF CSI (default: enabled).
/// - `--disable-htltf`       — Disable HTLTF CSI (default: enabled).
/// - `--disable-stbc-htltf`  — Disable STBC HTLTF CSI (default: enabled).
/// - `--disable-ltf-merge`   — Disable LTF Merge (default: enabled).
///
/// Prints the updated CSI configuration after applying changes.
pub fn set_csi<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let disable_lltf = argument_finder(item, args, "disable-lltf");
    let disable_htltf = argument_finder(item, args, "disable-htltf");
    let disable_stbc_htltf = argument_finder(item, args, "disable-stbc-htltf");
    let disable_ltf_merge = argument_finder(item, args, "disable-ltf-merge");

    match disable_lltf {
        Ok(str) => {
            if str.is_some() {
                USER_CONFIG.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .lltf_en = false;
                })
            }
        }
        Err(_) => (),
    }
    match disable_htltf {
        Ok(str) => {
            if str.is_some() {
                USER_CONFIG.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .htltf_en = false;
                })
            }
        }
        Err(_) => (),
    }
    match disable_stbc_htltf {
        Ok(str) => {
            if str.is_some() {
                USER_CONFIG.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .stbc_htltf2_en = false;
                })
            }
        }
        Err(_) => (),
    }
    match disable_ltf_merge {
        Ok(str) => {
            if str.is_some() {
                USER_CONFIG.lock(|config| {
                    config
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .csi_config
                        .ltf_merge_en = false;
                })
            }
        }
        Err(_) => (),
    }

    writeln!(serial, "\nUpdated CSI Configuration:\n").unwrap();
    USER_CONFIG.lock(|config| {
        writeln!(
            serial,
            "LLTF Enabled: {}",
            config.borrow().as_ref().unwrap().csi_config.lltf_en
        )
        .unwrap();
        writeln!(
            serial,
            "HTLTF Enabled: {}",
            config.borrow().as_ref().unwrap().csi_config.htltf_en
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
                .stbc_htltf2_en
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
                .ltf_merge_en
        )
        .unwrap();
    });
}

/// CLI command: `set-wifi`
///
/// Configures WiFi/radio operating parameters stored in [`USER_CONFIG`].
///
/// # Options
/// - `--mode=<station|sniffer|esp-now-central|esp-now-peripheral>` — Operating mode.
/// - `--sta-ssid=<SSID>` — SSID for Station mode. Wrap in `'...'` or `"..."` to include spaces; underscores are passed through literally.
/// - `--sta-password=<PASSWORD>` — Password for Station mode. Same quoting rules as `--sta-ssid`.
/// - `--set-channel=<NUMBER>` — WiFi channel (1–14).
///
/// Prints the updated WiFi configuration after applying changes.
pub fn set_wifi<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let mode = argument_finder(item, args, "mode");
    let sta_ssid = argument_finder(item, args, "sta-ssid");
    let sta_password = argument_finder(item, args, "sta-password");
    let set_channel = argument_finder(item, args, "set-channel");
    let peer_mac = argument_finder(item, args, "peer-mac");
    let ht40 = argument_finder(item, args, "ht40");

    match mode {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap() {
                    "sniffer" => USER_CONFIG.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().node_mode = NodeMode::WifiSniffer;
                    }),
                    "station" => USER_CONFIG.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().node_mode = NodeMode::WifiStation;
                    }),
                    "esp-now-central" => USER_CONFIG.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().node_mode = NodeMode::EspNowCentral;
                    }),
                    "esp-now-peripheral" => USER_CONFIG.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().node_mode = NodeMode::EspNowPeripheral;
                    }),
                    _ => writeln!(serial, "Invalid WiFi Mode").unwrap(),
                }
            }
        }
        Err(_) => (),
    }
    match set_channel {
        Ok(str) => {
            if str.is_some() {
                match str.unwrap().parse::<u8>() {
                    Ok(chan) => USER_CONFIG.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().channel = chan;
                    }),
                    Err(_) => writeln!(serial, "Invalid Max Connections").unwrap(),
                }
            }
        }
        Err(_) => (),
    }
    match sta_ssid {
        Ok(str) => {
            if let Some(s) = str {
                // 0x1F is the sentinel used by main.rs's quote-aware
                // preprocessor for spaces inside `'...'` / `"..."`. Underscores
                // are passed through literally.
                let str_w_space = s.replace('\u{1F}', " ");
                let mut hpls_str_w_space = heapless::String::<32>::new();
                hpls_str_w_space.push_str(&str_w_space).unwrap();

                USER_CONFIG.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().sta_ssid =
                        hpls_str_w_space.try_into().unwrap();
                });
            }
        }
        Err(_) => (),
    }
    match sta_password {
        Ok(str) => {
            if let Some(s) = str {
                let str_w_space = s.replace('\u{1F}', " ");
                let mut hpls_str_w_space = heapless::String::<32>::new();
                hpls_str_w_space.push_str(&str_w_space).unwrap();

                USER_CONFIG.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().sta_password = hpls_str_w_space
                });
            }
        }
        Err(_) => (),
    }
    // ESP-NOW explicit peer MAC. An empty value clears it (back to automatic
    // magic-prefix pairing); a valid MAC switches to per-node peer filtering.
    if let Ok(Some(s)) = peer_mac {
        if s.is_empty() {
            USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().peer_mac = None;
            });
        } else if let Some(mac) = parse_mac(s) {
            USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().peer_mac = Some(mac);
            });
        } else {
            writeln!(serial, "Invalid --peer-mac (use aa:bb:cc:dd:ee:ff)").unwrap();
        }
    }
    // ESP-NOW forced HT40 transmit PHY. `none` reverts to HT20/legacy per rate.
    if let Ok(Some(s)) = ht40 {
        match s {
            "above" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().ht40_secondary = Some(SecondaryChannel::Above);
            }),
            "below" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().ht40_secondary = Some(SecondaryChannel::Below);
            }),
            "none" | "off" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().ht40_secondary = None;
            }),
            _ => writeln!(serial, "Invalid --ht40 (use above|below|none)").unwrap(),
        }
    }

    writeln!(serial, "\nUpdated WiFi Configuration:\n").unwrap();
    USER_CONFIG.lock(|config| {
        writeln!(
            serial,
            "WiFi Mode: {:?}",
            config.borrow().as_ref().unwrap().node_mode
        )
        .unwrap();
        writeln!(
            serial,
            "WiFi Channel: {:?}",
            config.borrow().as_ref().unwrap().channel
        )
        .unwrap();
        writeln!(
            serial,
            "Station WiFi Settings:\nSSID: '{}', Password: '{}'",
            config.borrow().as_ref().unwrap().sta_ssid,
            config.borrow().as_ref().unwrap().sta_password,
        )
        .unwrap();
        let cfg = config.borrow();
        let cfg = cfg.as_ref().unwrap();
        match cfg.peer_mac {
            Some(m) => writeln!(
                serial,
                "ESP-NOW Peer MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                m[0], m[1], m[2], m[3], m[4], m[5]
            )
            .unwrap(),
            None => writeln!(serial, "ESP-NOW Peer MAC: auto").unwrap(),
        }
        let ht40_str = match cfg.ht40_secondary {
            Some(SecondaryChannel::Above) => "HT40 (secondary above)",
            Some(SecondaryChannel::Below) => "HT40 (secondary below)",
            _ => "HT20/legacy",
        };
        writeln!(serial, "ESP-NOW TX PHY: {}", ht40_str).unwrap();
    });
}

/// CLI command: `start`
///
/// Initiates a CSI collection run by signalling the [`csi_collection`] embassy task.
/// Sets [`IS_COLLECTING`] to lock the CLI until collection completes.
///
/// # Options
/// - `--duration=<SECONDS>` — Run for a fixed number of seconds. If omitted, runs indefinitely.
///
/// # Behaviour
/// Sends `Some(secs)` to [`START_SIGNAL`] for a timed run, or `None` for an indefinite run.
/// Pressing `q` or `Q` on the serial console while collection is active triggers an
/// early stop via [`STOP_REQUEST`].
pub fn start_csi_collect<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let duration = argument_finder(item, args, "duration");
    let signal_val = match duration {
        Ok(str) => {
            if let Some(s) = str {
                match s.parse::<u64>() {
                    Ok(secs) => {
                        writeln!(serial, "Starting CSI collection for {}s...", secs).unwrap();
                        Some(secs)
                    }
                    Err(_) => {
                        writeln!(serial, "Invalid duration").unwrap();
                        return;
                    }
                }
            } else {
                writeln!(serial, "Starting CSI collection indefinitely...").unwrap();
                None
            }
        }
        Err(_) => {
            writeln!(serial, "Starting CSI collection indefinitely...").unwrap();
            None
        }
    };
    IS_COLLECTING.store(true, Ordering::Relaxed);
    START_SIGNAL.signal(signal_val);
}

/// CLI command: `set-collection-mode`
///
/// Sets the CSI node collection role stored in [`USER_CONFIG`].
///
/// # Options
/// - `--mode=collector` — Node actively generates and collects CSI data (default).
/// - `--mode=listener`  — Node passively receives CSI data only.
///
/// Prints the updated mode after applying the change.
pub fn set_collection_mode<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let mode = argument_finder(item, args, "mode");
    match mode {
        Ok(Some(s)) => match s {
            "collector" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().collection_mode =
                    esp_csi_rs::CollectionMode::Collector;
            }),
            "listener" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().collection_mode =
                    esp_csi_rs::CollectionMode::Listener;
            }),
            _ => {
                writeln!(serial, "Invalid mode. Use 'collector' or 'listener'.").unwrap();
                return;
            }
        },
        _ => {
            writeln!(serial, "Usage: set-collection-mode --mode=<collector|listener>").unwrap();
            return;
        }
    }

    USER_CONFIG.lock(|config| {
        let mode_str = match config.borrow().as_ref().unwrap().collection_mode {
            esp_csi_rs::CollectionMode::Collector => "Collector",
            esp_csi_rs::CollectionMode::Listener => "Listener",
        };
        writeln!(serial, "\nCollection Mode: {}", mode_str).unwrap();
    });
}

/// CLI command: `set-log-mode`
///
/// Changes the CSI packet output format at runtime by calling
/// [`esp_csi_rs::logging::logging::set_log_mode`]. The change takes effect
/// immediately for subsequent packets logged by the `esp-csi-rs` logger
/// backend (sync or async-print).
///
/// # Options
/// - `--mode=text`          — Verbose human-readable output with full metadata (default).
/// - `--mode=array-list`    — Compact CSV-style array, one line per packet.
/// - `--mode=serialized`    — Binary COBS-framed postcard format for host-side parsing.
/// - `--mode=esp-csi-tool`  — Hernandez-style 26-column CSV (compatible with the ESP32-CSI-Tool collector).
///
/// Prints the updated mode after applying the change.
pub fn set_log_mode<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let mode = argument_finder(item, args, "mode");
    let mode_str = match mode {
        Ok(Some(s)) => match s {
            "text" => {
                csi_set_log_mode(LogMode::Text);
                "Text"
            }
            "array-list" => {
                csi_set_log_mode(LogMode::ArrayList);
                "ArrayList"
            }
            "serialized" => {
                csi_set_log_mode(LogMode::Serialized);
                "Serialized"
            }
            "esp-csi-tool" => {
                csi_set_log_mode(LogMode::EspCsiTool);
                "EspCsiTool"
            }
            _ => {
                writeln!(
                    serial,
                    "Invalid mode. Use 'text', 'array-list', 'serialized', or 'esp-csi-tool'."
                )
                .unwrap();
                return;
            }
        },
        _ => {
            writeln!(
                serial,
                "Usage: set-log-mode --mode=<text|array-list|serialized|esp-csi-tool>"
            )
            .unwrap();
            return;
        }
    };
    writeln!(serial, "\nLog Mode: {}", mode_str).unwrap();
}

/// CLI command: `show-config`
///
/// Prints a formatted summary of the current [`USER_CONFIG`] to the serial interface,
/// grouped into three sections: `[WiFi]`, `[Collection]`, and `[CSI Config]`.
///
/// CSI Config fields are platform-specific: ESP32-C6 exposes HE/STBC fields
/// while all other targets expose classic LLTF/HTLTF fields.
pub fn show_config<'a>(
    _menu: &Menu<SerialInterface, Context>,
    _item: &Item<SerialInterface, Context>,
    _args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    USER_CONFIG.lock(|config| {
        let cfg = config.borrow();
        let cfg = cfg.as_ref().unwrap();

        writeln!(serial, "\n====== Current Configuration ======\n").unwrap();

        // Node / WiFi settings
        writeln!(serial, "[WiFi]").unwrap();
        writeln!(serial, "  Mode    : {:?}", cfg.node_mode).unwrap();
        writeln!(serial, "  Channel : {}", cfg.channel).unwrap();
        writeln!(serial, "  STA SSID: '{}'", cfg.sta_ssid).unwrap();
        writeln!(serial, "  STA Pass: '{}'", cfg.sta_password).unwrap();
        match cfg.peer_mac {
            Some(m) => writeln!(
                serial,
                "  Peer MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                m[0], m[1], m[2], m[3], m[4], m[5]
            )
            .unwrap(),
            None => writeln!(serial, "  Peer MAC: auto").unwrap(),
        }
        let ht40_str = match cfg.ht40_secondary {
            Some(SecondaryChannel::Above) => "HT40 (secondary above)",
            Some(SecondaryChannel::Below) => "HT40 (secondary below)",
            _ => "HT20/legacy",
        };
        writeln!(serial, "  TX PHY  : {}", ht40_str).unwrap();

        // Collection settings
        writeln!(serial, "\n[Collection]").unwrap();
        let mode_str = match cfg.collection_mode {
            esp_csi_rs::CollectionMode::Collector => "Collector",
            esp_csi_rs::CollectionMode::Listener => "Listener",
        };
        writeln!(serial, "  Mode          : {}", mode_str).unwrap();
        writeln!(serial, "  Traffic Freq  : {}Hz", cfg.trigger_freq).unwrap();
        writeln!(serial, "  PHY Rate      : {:?}", cfg.phy_rate).unwrap();
        writeln!(serial, "  Protocol      : {:?}", cfg.protocol).unwrap();
        writeln!(
            serial,
            "  IO Tasks      : tx={}, rx={}",
            cfg.io_tasks.tx_enabled, cfg.io_tasks.rx_enabled
        )
        .unwrap();

        // CSI configuration (platform-specific fields)
        writeln!(serial, "\n[CSI Config]").unwrap();
        #[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
        {
            writeln!(serial, "  Acquire CSI        : {}", cfg.csi_config.enable).unwrap();
            writeln!(serial, "  Legacy (11g)       : {}", cfg.csi_config.acquire_csi_legacy).unwrap();
            writeln!(serial, "  HT20               : {}", cfg.csi_config.acquire_csi_ht20).unwrap();
            writeln!(serial, "  HT40               : {}", cfg.csi_config.acquire_csi_ht40).unwrap();
            writeln!(serial, "  HE20 SU            : {}", cfg.csi_config.acquire_csi_su).unwrap();
            writeln!(serial, "  HE20 MU            : {}", cfg.csi_config.acquire_csi_mu).unwrap();
            writeln!(serial, "  HE20 DCM           : {}", cfg.csi_config.acquire_csi_dcm).unwrap();
            writeln!(serial, "  HE20 Beamformed    : {}", cfg.csi_config.acquire_csi_beamformed).unwrap();
            writeln!(serial, "  STBC HE            : {}", cfg.csi_config.acquire_csi_he_stbc).unwrap();
            writeln!(serial, "  Scale Value        : {}", cfg.csi_config.val_scale_cfg).unwrap();
        }
        #[cfg(not(any(feature = "esp32c5", feature = "esp32c6")))]
        {
            writeln!(serial, "  LLTF Enabled       : {}", cfg.csi_config.lltf_en).unwrap();
            writeln!(serial, "  HTLTF Enabled      : {}", cfg.csi_config.htltf_en).unwrap();
            writeln!(serial, "  STBC HTLTF Enabled : {}", cfg.csi_config.stbc_htltf2_en).unwrap();
            writeln!(serial, "  LTF Merge Enabled  : {}", cfg.csi_config.ltf_merge_en).unwrap();
            writeln!(serial, "  Channel Filter     : {}", cfg.csi_config.channel_filter_en).unwrap();
            writeln!(serial, "  Manual Scale       : {}", cfg.csi_config.manu_scale).unwrap();
            writeln!(serial, "  Shift Bits         : {}", cfg.csi_config.shift).unwrap();
            writeln!(serial, "  Dump ACK           : {}", cfg.csi_config.dump_ack_en).unwrap();
        }

        writeln!(serial, "\n===================================\n").unwrap();
    });
}

/// CLI command: `reset-config`
///
/// Replaces the current [`USER_CONFIG`] with a fresh [`UserConfig::new`] instance,
/// restoring all settings to their compiled-in defaults.
///
/// Prints a confirmation message after the reset.
pub fn reset_config<'a>(
    _menu: &Menu<SerialInterface, Context>,
    _item: &Item<SerialInterface, Context>,
    _args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    USER_CONFIG.lock(|config: &RefCell<Option<_>>| {
        let default_config = UserConfig::new();
        config.replace(Some(default_config));
    });
    writeln!(serial, "\nConfiguration Reset to Default Values\n").unwrap();
}

/// CLI command: `set-rate`
///
/// Sets the Wi-Fi PHY rate stored in [`USER_CONFIG`]. Only ESP-NOW central /
/// peripheral modes apply this; sniffer/station derive their rate from the
/// surrounding radio configuration.
///
/// # Options
/// - `--rate=<NAME>` — One of: `mcs0-lgi`, `mcs1-lgi`, `mcs2-lgi`, `mcs3-lgi`,
///   `mcs4-lgi`, `mcs5-lgi`, `mcs6-lgi`, `mcs7-lgi`, `mcs0-sgi`, `1m`, `2m`,
///   `5m5`, `11m`, `6m`, `9m`, `12m`, `18m`, `24m`, `36m`, `48m`, `54m`.
pub fn set_phy_rate<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let rate = argument_finder(item, args, "rate");
    let parsed = match rate {
        Ok(Some(s)) => match s {
            "1m" | "1m-l" => Some(WifiPhyRate::Rate1mL),
            "2m" => Some(WifiPhyRate::Rate2m),
            "5m5" | "5m5-l" => Some(WifiPhyRate::Rate5mL),
            "11m" | "11m-l" => Some(WifiPhyRate::Rate11mL),
            "6m" => Some(WifiPhyRate::Rate6m),
            "9m" => Some(WifiPhyRate::Rate9m),
            "12m" => Some(WifiPhyRate::Rate12m),
            "18m" => Some(WifiPhyRate::Rate18m),
            "24m" => Some(WifiPhyRate::Rate24m),
            "36m" => Some(WifiPhyRate::Rate36m),
            "48m" => Some(WifiPhyRate::Rate48m),
            "54m" => Some(WifiPhyRate::Rate54m),
            "mcs0-lgi" => Some(WifiPhyRate::RateMcs0Lgi),
            "mcs1-lgi" => Some(WifiPhyRate::RateMcs1Lgi),
            "mcs2-lgi" => Some(WifiPhyRate::RateMcs2Lgi),
            "mcs3-lgi" => Some(WifiPhyRate::RateMcs3Lgi),
            "mcs4-lgi" => Some(WifiPhyRate::RateMcs4Lgi),
            "mcs5-lgi" => Some(WifiPhyRate::RateMcs5Lgi),
            "mcs6-lgi" => Some(WifiPhyRate::RateMcs6Lgi),
            "mcs7-lgi" => Some(WifiPhyRate::RateMcs7Lgi),
            "mcs0-sgi" => Some(WifiPhyRate::RateMcs0Sgi),
            _ => None,
        },
        _ => {
            writeln!(serial, "Usage: set-rate --rate=<rate>").unwrap();
            return;
        }
    };
    match parsed {
        Some(r) => {
            USER_CONFIG.lock(|c| c.borrow_mut().as_mut().unwrap().phy_rate = r);
            writeln!(serial, "\nPHY Rate: {:?}", r).unwrap();
        }
        None => {
            writeln!(
                serial,
                "Invalid rate. Try mcs0-lgi (default), mcs7-lgi, 6m, 24m, 54m, etc."
            )
            .unwrap();
        }
    }
}

/// CLI command: `set-protocol`
///
/// Sets the Wi-Fi PHY protocol stored in [`USER_CONFIG`] and applied to the
/// node via `CSINode::set_protocol` at the start of each collection run.
///
/// # Options
/// - `--protocol=<NAME>` — One of: `b`, `g`, `n`, `lr`, `a`, `ac`, `ax`.
///
/// `lr` (Espressif long-range) is the default and suits sniffer / ESP-NOW links
/// between ESP devices; use `n` (or `ax` on Wi-Fi 6 parts) when associating to a
/// standard AP in station mode.
pub fn set_protocol_cmd<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let protocol = argument_finder(item, args, "protocol");
    let parsed = match protocol {
        Ok(Some(s)) => match s {
            "b" => Some(Protocol::B),
            "g" => Some(Protocol::G),
            "n" => Some(Protocol::N),
            "lr" => Some(Protocol::LR),
            "a" => Some(Protocol::A),
            "ac" => Some(Protocol::AC),
            "ax" => Some(Protocol::AX),
            _ => None,
        },
        _ => {
            writeln!(serial, "Usage: set-protocol --protocol=<b|g|n|lr|a|ac|ax>").unwrap();
            return;
        }
    };
    match parsed {
        Some(p) => {
            USER_CONFIG.lock(|c| c.borrow_mut().as_mut().unwrap().protocol = p);
            writeln!(serial, "\nProtocol: {:?}", p).unwrap();
        }
        None => {
            writeln!(
                serial,
                "Invalid protocol. Use one of: b, g, n, lr (default), a, ac, ax."
            )
            .unwrap();
        }
    }
}

/// CLI command: `set-io-tasks`
///
/// Toggles TX and/or RX direction tasks via [`IOTaskConfig`]. Useful for
/// asymmetric topologies where one node only generates traffic and another
/// only receives.
///
/// # Options
/// - `--tx=<on|off>` — Enable or disable the TX task. Omit to keep current.
/// - `--rx=<on|off>` — Enable or disable the RX task. Omit to keep current.
pub fn set_io_tasks_cmd<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    fn parse_bool(s: &str) -> Option<bool> {
        match s {
            "on" | "true" | "1" | "yes" => Some(true),
            "off" | "false" | "0" | "no" => Some(false),
            _ => None,
        }
    }
    let tx = argument_finder(item, args, "tx");
    let rx = argument_finder(item, args, "rx");
    USER_CONFIG.lock(|cfg| {
        let mut cfg = cfg.borrow_mut();
        let cfg = cfg.as_mut().unwrap();
        if let Ok(Some(s)) = tx {
            match parse_bool(s) {
                Some(v) => cfg.io_tasks.tx_enabled = v,
                None => writeln!(serial, "Invalid --tx value (use on|off).").unwrap(),
            }
        }
        if let Ok(Some(s)) = rx {
            match parse_bool(s) {
                Some(v) => cfg.io_tasks.rx_enabled = v,
                None => writeln!(serial, "Invalid --rx value (use on|off).").unwrap(),
            }
        }
        writeln!(
            serial,
            "\nIO Tasks: tx={}, rx={}",
            cfg.io_tasks.tx_enabled, cfg.io_tasks.rx_enabled
        )
        .unwrap();
    });
}

/// CLI command: `set-csi-delivery`
///
/// Switches the CSI delivery mode at runtime (Off, Callback, or Async). The
/// inline log gate is also toggled with `--logging=on|off` if supplied.
///
/// # Options
/// - `--mode=<off|callback|async>` — Pick how the WiFi callback hands CSI
///   packets out. `async` queues to `CSINodeClient::next_csi_packet`,
///   `callback` invokes the inline `set_csi_callback` hook, `off` drops
///   the dispatch entirely.
/// - `--logging=<on|off>` — Toggle the inline `log_csi` per-packet UART/JTAG
///   path independently of the delivery mode.
pub fn set_csi_delivery_cmd<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let mode = argument_finder(item, args, "mode");
    if let Ok(Some(s)) = mode {
        // `raw` selects the zero-copy fast-path, which is registered by the
        // collection task at the next `start` (no `CsiDeliveryMode` variant
        // exists for it). The other modes take effect immediately via
        // `set_csi_delivery_mode` and also clear the raw flag.
        let set_raw = |raw: bool| {
            USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().delivery_raw = raw;
            });
        };
        match s {
            "off" => {
                set_raw(false);
                set_csi_delivery_mode(CsiDeliveryMode::Off);
                writeln!(serial, "Delivery mode: Off").unwrap();
            }
            "callback" => {
                set_raw(false);
                set_csi_delivery_mode(CsiDeliveryMode::Callback);
                writeln!(serial, "Delivery mode: Callback").unwrap();
            }
            "async" => {
                set_raw(false);
                set_csi_delivery_mode(CsiDeliveryMode::Async);
                writeln!(serial, "Delivery mode: Async").unwrap();
            }
            "raw" => {
                set_raw(true);
                writeln!(
                    serial,
                    "Delivery mode: Raw (zero-copy fast-path; applies on next start, no CSI data delivered)"
                )
                .unwrap();
            }
            _ => {
                writeln!(serial, "Invalid mode. Use 'off', 'callback', 'async', or 'raw'.").unwrap();
            }
        }
    }
    let logging = argument_finder(item, args, "logging");
    if let Ok(Some(s)) = logging {
        match s {
            "on" | "true" | "1" | "yes" => {
                set_csi_logging_enabled(true);
                writeln!(serial, "Inline CSI logging: ON").unwrap();
            }
            "off" | "false" | "0" | "no" => {
                set_csi_logging_enabled(false);
                writeln!(serial, "Inline CSI logging: OFF").unwrap();
            }
            _ => {
                writeln!(serial, "Invalid --logging value (use on|off).").unwrap();
            }
        }
    }
}

/// CLI command: `info`
///
/// Prints a machine-parseable firmware identification block for host-side
/// tooling. The first line is the magic string `ESP-CSI-CLI/<version>` (also
/// emitted at the top of the welcome banner on reset). Subsequent lines are
/// `key=value` pairs terminated by a sentinel `END-INFO` line.
///
/// Format (stable; bump [`CLI_PROTOCOL_VERSION`] on breaking changes):
///
/// ```text
/// ESP-CSI-CLI/<version>
/// name=esp-csi-cli-rs
/// version=<version>
/// chip=<esp32|esp32c3|esp32c5|esp32c6|esp32s3|unknown>
/// protocol=<u32>
/// mac=<AA:BB:CC:DD:EE:FF>
/// features=<comma-separated-list>
/// END-INFO
/// ```
///
/// The `mac` field (protocol >= 2) is the factory eFuse MAC, which is also the
/// USB `iSerialNumber` on native-USB boards. Host tooling uses it as the stable
/// device key so a `restart`/re-enumeration re-binds to the same board.
pub fn cli_info<'a>(
    _menu: &Menu<SerialInterface, Context>,
    _item: &Item<SerialInterface, Context>,
    _args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let version = env!("CARGO_PKG_VERSION");
    let name = env!("CARGO_PKG_NAME");

    let chip = if cfg!(feature = "esp32") {
        "esp32"
    } else if cfg!(feature = "esp32c3") {
        "esp32c3"
    } else if cfg!(feature = "esp32c5") {
        "esp32c5"
    } else if cfg!(feature = "esp32c6") {
        "esp32c6"
    } else if cfg!(feature = "esp32s3") {
        "esp32s3"
    } else {
        "unknown"
    };

    let mut features: heapless::String<128> = heapless::String::new();
    macro_rules! push_feat {
        ($name:literal) => {
            if cfg!(feature = $name) {
                if !features.is_empty() {
                    let _ = features.push(',');
                }
                let _ = features.push_str($name);
            }
        };
    }
    push_feat!("statistics");
    push_feat!("defmt");
    push_feat!("println");
    push_feat!("async-print");
    push_feat!("auto");
    push_feat!("jtag-serial");
    push_feat!("uart");

    let mac = device_mac();

    writeln!(serial, "ESP-CSI-CLI/{}", version).unwrap();
    writeln!(serial, "name={}", name).unwrap();
    writeln!(serial, "version={}", version).unwrap();
    writeln!(serial, "chip={}", chip).unwrap();
    writeln!(serial, "protocol={}", CLI_PROTOCOL_VERSION).unwrap();
    writeln!(
        serial,
        "mac={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
    .unwrap();
    writeln!(serial, "features={}", features).unwrap();
    writeln!(serial, "END-INFO").unwrap();
}

/// CLI command: `restart`
///
/// Performs a clean software reset of the chip via
/// [`esp_hal::system::software_reset`]. This is the device half of the
/// native-USB reset story: on boards whose serial transport is the built-in
/// USB-Serial-JTAG peripheral, resetting drops and re-enumerates the USB
/// device. Because host tooling pins to the board's MAC (see [`device_mac`] /
/// the `mac=` field of `info`) rather than the `/dev/ttyACM*` path, the
/// re-enumeration is a non-event — the per-device task re-binds to the same
/// physical board on the new node number.
///
/// The function diverges (`software_reset` returns `!`); the firmware reboots
/// and re-emits the welcome banner — including the magic identification line
/// and `mac=` — which the host greps to confirm the board is back.
pub fn restart_cmd<'a>(
    _menu: &Menu<SerialInterface, Context>,
    _item: &Item<SerialInterface, Context>,
    _args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    writeln!(serial, "\nRestarting...").unwrap();
    // Push the message out of the peripheral FIFO before the reset tears the
    // transport down, otherwise it can be lost on the way out.
    let _ = Write::flush(serial);
    esp_hal::system::software_reset();
}

/// CLI command: `show-stats` (compiled in only with the `statistics` feature)
///
/// Reads the runtime CSI/traffic counters exposed by `esp-csi-rs` and prints
/// a one-shot snapshot. These counters are accumulated by the WiFi callback
/// and the ESP-NOW TX path; they are most meaningful while a collection is
/// running, but they remain queryable between runs (the values reset on the
/// start of each new `start`).
#[cfg(feature = "statistics")]
pub fn show_stats<'a>(
    _menu: &Menu<SerialInterface, Context>,
    _item: &Item<SerialInterface, Context>,
    _args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    writeln!(serial, "\n====== Runtime Statistics ======").unwrap();
    writeln!(serial, "  RX Total Packets : {}", get_total_rx_packets()).unwrap();
    writeln!(serial, "  TX Total Packets : {}", get_total_tx_packets()).unwrap();
    writeln!(serial, "  RX PPS (avg)     : {}", get_pps_rx()).unwrap();
    writeln!(serial, "  TX PPS (avg)     : {}", get_pps_tx()).unwrap();
    writeln!(serial, "  RX Rate (Hz)     : {}", get_rx_rate_hz()).unwrap();
    writeln!(serial, "  TX Rate (Hz)     : {}", get_tx_rate_hz()).unwrap();
    writeln!(serial, "  RX Dropped Pkts  : {}", get_dropped_packets_rx()).unwrap();
    writeln!(serial, "  TX Queued Pkts   : {}", get_tx_queued_packets()).unwrap();
    writeln!(serial, "  TX Confirmed Pkts: {}", get_tx_confirmed_packets()).unwrap();
    writeln!(serial, "  TX Failed Pkts   : {}", get_tx_failed_packets()).unwrap();
    writeln!(serial, "================================\n").unwrap();
}