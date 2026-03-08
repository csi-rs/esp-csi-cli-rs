use core::cell::RefCell;
use core::sync::atomic::Ordering;
use embedded_io::Write;
use esp_csi_rs::logging::logging::set_log_mode as csi_set_log_mode;
use esp_csi_rs::logging::logging::LogMode;

use menu::{Item, Menu, argument_finder};

use crate::{NodeMode, cli::{Context, SerialInterface}, config::{IS_COLLECTING, START_SIGNAL, USER_CONFIG, UserConfig}};

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

#[cfg(feature = "esp32c6")]
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

#[cfg(not(feature = "esp32c6"))]
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
/// - `--sta-ssid=<SSID>` — SSID for Station mode (replace spaces with `_`).
/// - `--sta-password=<PASSWORD>` — Password for Station mode (replace spaces with `_`).
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
                let str_w_space = s.replace("_", " ");
                // Convert the `mod_str` into a `heapless::String<32>`
                let mut hpls_str_w_space = heapless::String::<32>::new();
                hpls_str_w_space.push_str(&str_w_space).unwrap(); // Ensure it fits within the capacity

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
                let str_w_space = s.replace("_", " ");
                // Convert the `mod_str` into a `heapless::String<32>`
                let mut hpls_str_w_space = heapless::String::<32>::new();
                hpls_str_w_space.push_str(&str_w_space).unwrap(); // Ensure it fits within the capacity

                USER_CONFIG.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().sta_password = hpls_str_w_space
                });
            }
        }
        Err(_) => (),
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
/// [`esp_csi_rs::set_log_mode`]. The change takes effect immediately for
/// subsequent packets logged by the `esp-csi-rs` async logger backend.
///
/// # Options
/// - `--mode=text`        — Verbose human-readable output with full metadata (default).
/// - `--mode=array-list`  — Compact CSV-style array, one line per packet.
/// - `--mode=serialized`  — Binary COBS-framed postcard format for host-side parsing.
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
            _ => {
                writeln!(serial, "Invalid mode. Use 'text', 'array-list', or 'serialized'.").unwrap();
                return;
            }
        },
        _ => {
            writeln!(serial, "Usage: set-log-mode --mode=<text|array-list|serialized>").unwrap();
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

        // Collection settings
        writeln!(serial, "\n[Collection]").unwrap();
        let mode_str = match cfg.collection_mode {
            esp_csi_rs::CollectionMode::Collector => "Collector",
            esp_csi_rs::CollectionMode::Listener => "Listener",
        };
        writeln!(serial, "  Mode          : {}", mode_str).unwrap();
        writeln!(serial, "  Traffic Freq  : {}Hz", cfg.trigger_freq).unwrap();

        // CSI configuration (platform-specific fields)
        writeln!(serial, "\n[CSI Config]").unwrap();
        #[cfg(feature = "esp32c6")]
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
        #[cfg(not(feature = "esp32c6"))]
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