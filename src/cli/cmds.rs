use core::cell::RefCell;
use embedded_io::Write;

use menu::{Item, Menu, argument_finder};

use crate::{NodeMode, cli::{Context, SerialInterface}, config::{USER_CONFIG, UserConfig}};

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
pub fn set_csi<'a>(
    _menu: &Menu<SerialInterfaceType, Context>,
    item: &Item<SerialInterfaceType, Context>,
    args: &[&str],
    mut serial: &mut SerialInterfaceType,
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
                        .lltf_enabled = false;
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
                        .htltf_enabled = false;
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
                        .stbc_htltf2_enabled = false;
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
                        .ltf_merge_enabled = false;
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
            config.borrow().as_ref().unwrap().csi_config.lltf_enabled
        )
        .unwrap();
        writeln!(
            serial,
            "HTLTF Enabled: {}",
            config.borrow().as_ref().unwrap().csi_config.htltf_enabled
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
                .stbc_htltf2_enabled
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
                .ltf_merge_enabled
        )
        .unwrap();
    });
}

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

pub fn show_config<'a>(
    _menu: &Menu<SerialInterface, Context>,
    _item: &Item<SerialInterface, Context>,
    _args: &[&str],
    _serial: &mut SerialInterface,
    _context: &mut Context,
) {
}

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