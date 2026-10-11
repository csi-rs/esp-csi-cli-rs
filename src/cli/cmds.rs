use core::cell::RefCell;
use core::sync::atomic::Ordering;
use embedded_io::Write;
use esp_csi_rs::logging::logging::LogMode;
use esp_csi_rs::logging::logging::set_log_mode as csi_set_log_mode;
use esp_csi_rs::{
    CsiDeliveryMode, ReportTo, Reporting, SensingRole, SessionRole, set_csi_delivery_mode,
    set_csi_logging_enabled,
};
#[cfg(feature = "statistics")]
use esp_csi_rs::{
    get_drop_breakdown, get_dropped_packets_rx, get_pps_rx, get_pps_tx, get_rx_rate_hz,
    get_total_rx_packets, get_total_tx_packets, get_tx_rate_hz,
};
use esp_radio::esp_now::WifiPhyRate;
use esp_radio::wifi::{Protocol, SecondaryChannel};

use menu::{Item, Menu, argument_finder};

use crate::{
    NodeMode, build_mode,
    cli::{Context, SerialInterface},
    config::{IS_COLLECTING, START_SIGNAL, USER_CONFIG, UserConfig},
};

/// Wire-format version of the firmware identification block emitted by
/// [`cli_info`] and the welcome banner. Bump on any breaking change to the
/// `info` grammar so host-side tooling can refuse incompatible firmware.
///
/// v2: added the `mac=` line to `info` (and the welcome banner). Host tooling
/// pins per-device tasks to this stable serial number rather than the
/// `/dev/ttyACM*` path, so a `restart` and the USB re-enumeration it triggers
/// re-bind to the same physical board whatever device node it returns as.
///
/// v3: esp-csi-rs 0.12. `set-log-mode --mode=serialized` now emits the versioned
/// `esp_csi_rs::wire` format, which a v2 host decoder cannot read. The `info`
/// grammar itself is unchanged; the bump lets hosts refuse a decoder mismatch.
///
/// v4: esp-csi-rs 0.13, IEEE 802.11bf naming. `set-wifi --mode=` tokens are
/// `esp-now-initiator`/`esp-now-responder`/`esp-now-simplex-source`/`esp-now-simplex-peer`
/// (the central/peripheral/fast spellings are gone), `--collection=` is `--reporting=`,
/// `set-reporting --policy=` takes `report|none|threshold|decimate`, and `set-traffic
/// --frequency-hz=` is `set-measurement --periodicity-hz=`. A v3 host would send commands this
/// firmware rejects. The serialized CSI format is unchanged.
pub const CLI_PROTOCOL_VERSION: u32 = 4;

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

/// CLI command: `set-measurement`
///
/// Configures the node-level measurement setup stored in [`USER_CONFIG`].
///
/// # Options
/// - `--periodicity-hz=<NUMBER>` — rate of the sounding traffic the node generates, in Hz
///   (`MeasurementSetup::periodicity_hz`). `0` disables traffic generation.
/// - `--unsolicited=<on|off>` — ICMP flood sends unsolicited echo replies.
///
/// Prints the updated setup after applying the change.
pub fn set_measurement<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    if let Ok(Some(s)) = argument_finder(item, args, "periodicity-hz") {
        match s.parse::<u16>() {
            Ok(hz) => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().periodicity_hz = hz
            }),
            Err(_) => writeln!(serial, "Invalid --periodicity-hz (use 0-65535)").unwrap(),
        }
    }

    if let Ok(Some(s)) = argument_finder(item, args, "unsolicited") {
        match parse_on_off(s) {
            Some(v) => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().flood_unsolicited = v
            }),
            None => writeln!(serial, "Invalid --unsolicited value (use on|off).").unwrap(),
        }
    }

    writeln!(serial, "\nUpdated Measurement Setup:\n").unwrap();
    USER_CONFIG.lock(|config| {
        let config = config.borrow();
        let config = config.as_ref().unwrap();
        writeln!(
            serial,
            "Periodicity: {} Hz\nICMP Flood Kind: {}",
            config.periodicity_hz,
            if config.flood_unsolicited {
                "unsolicited echo replies (one-directional)"
            } else {
                "echo requests (peer replies)"
            },
        )
        .unwrap();
    });
}

/// Parse an on/off-style flag value used by the `set-csi` toggles.
///
/// Accepts `on|off`, `true|false`, `1|0`, `enable[d]|disable[d]`, `yes|no`.
/// Returns `None` for anything else so the caller can report the bad value.
fn parse_on_off(s: &str) -> Option<bool> {
    match s {
        "on" | "true" | "1" | "enable" | "enabled" | "yes" => Some(true),
        "off" | "false" | "0" | "disable" | "disabled" | "no" => Some(false),
        _ => None,
    }
}

/// Human-readable form of a reporting value: `report (host)`, `report (local)`, `none`,
/// `threshold (...)` or `decimate N`.
fn reporting_str(reporting: Reporting) -> heapless::String<64> {
    let mut out = heapless::String::new();
    let _ = match reporting {
        Reporting::Report { to: ReportTo::Local } => {
            core::fmt::Write::write_str(&mut out, "report (local)")
        }
        Reporting::Report { .. } => core::fmt::Write::write_str(&mut out, "report (host)"),
        Reporting::None => core::fmt::Write::write_str(&mut out, "none"),
        Reporting::ThresholdBased(t) => core::fmt::Write::write_fmt(
            &mut out,
            format_args!("threshold (level={}, hold={}ms)", t.level, t.hold_ms),
        ),
        Reporting::Decimate(n) => {
            core::fmt::Write::write_fmt(&mut out, format_args!("decimate {}", n))
        }
        _ => core::fmt::Write::write_str(&mut out, "other"),
    };
    out
}

/// The reporting the next run will use, as printed by `show-config`, `set-wifi` and
/// `set-reporting`. Read back from the same [`esp_csi_rs::Mode`] the run is built with.
///
/// ESP-NOW, station and access point take any value. A sniffer takes all but `none`. Every
/// other mode fixes the attribute, so the fixed value is shown rather than a setting it ignores.
fn reporting_line(cfg: &UserConfig) -> heapless::String<96> {
    let mut out: heapless::String<96> = heapless::String::new();
    let _ = out.push_str(&reporting_str(build_mode(cfg).reporting()));
    if !cfg.node_mode.reads_reporting() {
        let _ = out.push_str(" (fixed by mode)");
    } else if cfg.node_mode == NodeMode::WifiSniffer && cfg.reporting == Reporting::None {
        let _ = out.push_str(" (a sniffer cannot report none)");
    }
    out
}

fn session_role_str(role: Option<SessionRole>) -> &'static str {
    match role {
        Some(SessionRole::Initiator) => "initiator",
        Some(SessionRole::Responder) => "responder",
        Some(_) => "other",
        None => "none (observer)",
    }
}

fn sensing_role_str(role: SensingRole) -> &'static str {
    match role {
        SensingRole::Transmitter => "transmitter",
        SensingRole::Receiver => "receiver",
        SensingRole::TransmitterReceiver => "transmitter-receiver",
        _ => "other",
    }
}

/// Print the node's mode, roles and reporting: the lines `show-config` and `set-wifi` share.
///
/// The session role is a choice on the ESP-NOW pair (the `--mode` token picks the end) and on the
/// station (the initiator while `--periodicity-hz` > 0, otherwise an observer); every other mode,
/// and the sensing role everywhere, is fixed by the mode.
fn write_node_attributes(serial: &mut SerialInterface, cfg: &UserConfig, indent: &str) {
    let mode = build_mode(cfg);
    let session_fixed = !matches!(
        cfg.node_mode,
        NodeMode::EspNowInitiator | NodeMode::EspNowResponder | NodeMode::WifiStation
    );
    writeln!(serial, "{}Mode: {}", indent, cfg.node_mode.token()).unwrap();
    writeln!(
        serial,
        "{}Session role: {}{}",
        indent,
        session_role_str(mode.session_role()),
        if session_fixed { " (fixed by mode)" } else { "" }
    )
    .unwrap();
    writeln!(
        serial,
        "{}Sensing role: {} (fixed by mode)",
        indent,
        sensing_role_str(mode.sensing_role())
    )
    .unwrap();
    writeln!(serial, "{}Reporting: {}", indent, reporting_line(cfg)).unwrap();
}

/// Parse a `--reporting=` / `--policy=` value against the stored destination, threshold and
/// decimation parameters. No aliases: the pre-0.10 `collector|listener|always|never` are rejected.
fn parse_reporting(s: &str, cfg: &UserConfig) -> Option<Reporting> {
    match s.to_ascii_lowercase().as_str() {
        "report" => Some(Reporting::Report { to: cfg.report_to }),
        "none" => Some(Reporting::None),
        "threshold" => Some(Reporting::ThresholdBased(cfg.threshold)),
        "decimate" => Some(Reporting::Decimate(cfg.decimate_n)),
        _ => None,
    }
}

/// Parse a `--report-to=` value.
fn parse_report_to(s: &str) -> Option<ReportTo> {
    match s.to_ascii_lowercase().as_str() {
        "host" => Some(ReportTo::Host),
        "local" => Some(ReportTo::Local),
        _ => None,
    }
}

const SNIFFER_NONE_ERROR: &str =
    "Invalid --reporting=none for sniffer: a sniffer that reports nothing observes nothing (use report|threshold|decimate)";

/// Apply `--report-to=` and then a `--reporting=`/`--policy=` value (either may be absent) to the
/// config. `flag` names the reporting flag in error messages. Refuses `none` on a sniffer.
fn apply_reporting_flags(
    serial: &mut SerialInterface,
    cfg: &mut UserConfig,
    flag: &str,
    value: Option<&str>,
    report_to: Option<&str>,
) {
    if let Some(v) = report_to {
        match parse_report_to(v) {
            Some(to) => {
                cfg.report_to = to;
                // Re-derive an active `report` so the destination applies without restating it.
                if let Reporting::Report { .. } = cfg.reporting {
                    cfg.reporting = Reporting::Report { to };
                }
            }
            None => writeln!(serial, "Invalid --report-to (use host|local)").unwrap(),
        }
    }
    if let Some(v) = value {
        match parse_reporting(v, cfg) {
            Some(Reporting::None) if cfg.node_mode == NodeMode::WifiSniffer => {
                writeln!(serial, "{}", SNIFFER_NONE_ERROR).unwrap()
            }
            Some(r) => cfg.reporting = r,
            None => writeln!(
                serial,
                "Invalid --{} (use report|none|threshold|decimate)",
                flag
            )
            .unwrap(),
        }
    }
}

#[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
/// CLI command: `set-csi` (ESP32-C6 variant)
///
/// Configures the per-PPDU-format CSI acquisition flags in [`USER_CONFIG`].
/// Each acquisition flag is an explicit `on|off` toggle, so features can be
/// re-enabled after being turned off without a `reset-config`.
///
/// # Options
/// - `--csi=<on|off>`             — CSI acquisition (master switch).
/// - `--csi-legacy=<on|off>`      — L-LTF for 11g PPDUs.
/// - `--csi-ht20=<on|off>`        — HT-LTF for HT20 PPDUs.
/// - `--csi-ht40=<on|off>`        — HT-LTF for HT40 PPDUs.
/// - `--val-scale-cfg=<0-3>`      — Value scale configuration (default: 2).
/// - `--preset=<default>`         — Apply a CSI acquisition preset.
/// - `--csi-force-lltf=<on|off>`  — Force L-LTF acquisition (ESP32-C5 only).
/// - `--csi-vht=<on|off>`         — VHT-LTF for VHT20 PPDUs (ESP32-C5 only).
/// - `--csi-su` / `--csi-mu` / `--csi-dcm` / `--csi-beamformed` / `--csi-he-stbc=<on|off>`
///   — HE-LTF acquisition per HE PPDU kind.
/// - `--preset=<default|he20>`    — `he20` is HE-LTF only (`CsiConfig::he20()`).
/// - `--dump-ack=<on|off>`        — Dump 802.11 ACK frames (default: on).
///
/// Prints the updated CSI configuration after applying changes.
pub fn set_csi<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    // Apply a `--<name>=<on|off>` toggle to a `u32` (0/1) CsiConfig field.
    macro_rules! apply_flag {
        ($name:literal, $field:ident) => {
            if let Ok(Some(v)) = argument_finder(item, args, $name) {
                match parse_on_off(v) {
                    Some(b) => USER_CONFIG.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().csi_config.$field = b as u32;
                    }),
                    None => {
                        writeln!(serial, "Invalid --{} value '{}' (use on|off)", $name, v).unwrap()
                    }
                }
            }
        };
    }

    apply_flag!("csi", enable);
    apply_flag!("csi-legacy", acquire_csi_legacy);
    apply_flag!("csi-ht20", acquire_csi_ht20);
    apply_flag!("csi-ht40", acquire_csi_ht40);
    apply_flag!("dump-ack", dump_ack_en);
    apply_flag!("csi-su", acquire_csi_su);
    apply_flag!("csi-mu", acquire_csi_mu);
    apply_flag!("csi-dcm", acquire_csi_dcm);
    apply_flag!("csi-beamformed", acquire_csi_beamformed);
    apply_flag!("csi-he-stbc", acquire_csi_he_stbc);

    #[cfg(feature = "esp32c5")]
    {
        if let Ok(Some(v)) = argument_finder(item, args, "csi-force-lltf") {
            match parse_on_off(v) {
                Some(b) => USER_CONFIG.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().csi_config.acquire_csi_force_lltf =
                        b;
                }),
                None => writeln!(
                    serial,
                    "Invalid --csi-force-lltf value '{}' (use on|off)",
                    v
                )
                .unwrap(),
            }
        }
        if let Ok(Some(v)) = argument_finder(item, args, "csi-vht") {
            match parse_on_off(v) {
                Some(b) => USER_CONFIG.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().csi_config.acquire_csi_vht = b;
                }),
                None => writeln!(serial, "Invalid --csi-vht value '{}' (use on|off)", v).unwrap(),
            }
        }
    }

    if let Ok(Some(v)) = argument_finder(item, args, "preset") {
        match v {
            "default" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().csi_config =
                    esp_csi_rs::config::CsiConfig::default();
            }),
            "he20" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().csi_config =
                    esp_csi_rs::config::CsiConfig::he20();
            }),
            _ => writeln!(serial, "Invalid --preset value '{}' (use default|he20)", v).unwrap(),
        }
    }

    if let Ok(Some(v)) = argument_finder(item, args, "val-scale-cfg") {
        match v.parse::<u32>() {
            Ok(val) if val <= 3 => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().csi_config.val_scale_cfg = val;
            }),
            _ => writeln!(serial, "Invalid --val-scale-cfg value '{}' (use 0-3)", v).unwrap(),
        }
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
            "Scale Value: {}",
            config.borrow().as_ref().unwrap().csi_config.val_scale_cfg
        )
        .unwrap();
        writeln!(
            serial,
            "Dump ACK: {}",
            config.borrow().as_ref().unwrap().csi_config.dump_ack_en
        )
        .unwrap();
        {
            let cfg = config.borrow();
            let c = &cfg.as_ref().unwrap().csi_config;
            writeln!(
                serial,
                "HE-LTF: SU={} MU={} DCM={} Beamformed={} HE-STBC={}",
                c.acquire_csi_su,
                c.acquire_csi_mu,
                c.acquire_csi_dcm,
                c.acquire_csi_beamformed,
                c.acquire_csi_he_stbc
            )
            .unwrap();
        }
        #[cfg(feature = "esp32c5")]
        {
            writeln!(
                serial,
                "Force L-LTF: {}",
                config
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .csi_config
                    .acquire_csi_force_lltf
            )
            .unwrap();
            writeln!(
                serial,
                "VHT20: {}",
                config.borrow().as_ref().unwrap().csi_config.acquire_csi_vht
            )
            .unwrap();
        }
    });
}

#[cfg(not(any(feature = "esp32c5", feature = "esp32c6")))]
/// CLI command: `set-csi` (non-ESP32-C6 variant)
///
/// Configures classic CSI acquisition feature flags in [`USER_CONFIG`]. Each
/// flag is an explicit `on|off` toggle, so features can be re-enabled after
/// being turned off without a `reset-config`.
///
/// # Options
/// - `--lltf=<on|off>`        — LLTF CSI (default: on).
/// - `--htltf=<on|off>`       — HTLTF CSI (default: on).
/// - `--stbc-htltf=<on|off>`  — STBC HTLTF CSI (default: on).
/// - `--ltf-merge=<on|off>`   — LTF Merge (default: on).
///
/// Prints the updated CSI configuration after applying changes.
pub fn set_csi<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    // Apply a `--<name>=<on|off>` toggle to a `bool` CsiConfig field.
    macro_rules! apply_flag {
        ($name:literal, $field:ident) => {
            if let Ok(Some(v)) = argument_finder(item, args, $name) {
                match parse_on_off(v) {
                    Some(b) => USER_CONFIG.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().csi_config.$field = b;
                    }),
                    None => {
                        writeln!(serial, "Invalid --{} value '{}' (use on|off)", $name, v).unwrap()
                    }
                }
            }
        };
    }

    apply_flag!("lltf", lltf_en);
    apply_flag!("htltf", htltf_en);
    apply_flag!("stbc-htltf", stbc_htltf2_en);
    apply_flag!("ltf-merge", ltf_merge_en);

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
            config.borrow().as_ref().unwrap().csi_config.stbc_htltf2_en
        )
        .unwrap();
        writeln!(
            serial,
            "LTF Merge Enabled: {}",
            config.borrow().as_ref().unwrap().csi_config.ltf_merge_en
        )
        .unwrap();
    });
}

/// CLI command: `set-wifi`
///
/// Configures WiFi/radio operating parameters stored in [`USER_CONFIG`].
///
/// # Options
/// - `--mode=<station|sniffer|wifi-ap|ht20-emitter|ht40-emitter|esp-now-initiator|esp-now-responder|esp-now-simplex-source|esp-now-simplex-peer>` — Mode.
/// - `--sta-ssid=<SSID>` — SSID for Station mode. Wrap in `'...'` or `"..."` to include spaces; underscores are passed through literally.
/// - `--sta-password=<PASSWORD>` — Password for Station mode. Same quoting rules as `--sta-ssid`.
/// - `--ap-ssid=<SSID>` — SSID for wifi-ap mode. Same quoting rules as `--sta-ssid`.
/// - `--ap-password=<PASSWORD>` — Password for wifi-ap mode (empty = open). Same quoting rules.
/// - `--ap-dhcp=<on|off>` — Enable/disable the built-in DHCP server in wifi-ap mode.
/// - `--ap-leases=<1-8>` — DHCP lease pool size in wifi-ap mode. With more than
///   one lease the ICMP flood round-robins across all associated stations.
/// - `--ap-burst=<on|off>` — Synchronized burst flood in wifi-ap mode. Every
///   tick sends one frame back-to-back to every associated station for
///   time-aligned multi-receiver CSI (total airtime = periodicity-hz × leases).
/// - `--set-channel=<NUMBER>` — WiFi channel (1–14; the ESP32-C5 also takes 5 GHz channels).
/// - `--peer-mac=<aa:bb:cc:dd:ee:ff>` — emitter injection destination, or explicit ESP-NOW peer
///   (all ESP-NOW modes, simplex included; set it on both nodes).
/// - `--inject-period-ms=<MS>` / `--inject-period-us=<US>` — emitter inter-frame period
///   (default 20000 us). When both are given the µs value wins.
/// - `--emitter-iface=<sta|ap>` — interface an emitter injects on.
/// - `--ht40=<above|below|none>` — softAP secondary channel in `wifi-ap`, and the forced per-peer
///   TX PHY in the ESP-NOW modes.
/// - `--reporting=<report|none|threshold|decimate>` — the node's reporting. Read only by the modes
///   that admit a choice (ESP-NOW initiator/responder, station, wifi-ap, and the sniffer, which
///   refuses `none`); the others fix it.
/// - `--report-to=<host|local>` — where `report` sends measurements (default host).
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
    let ap_ssid = argument_finder(item, args, "ap-ssid");
    let ap_password = argument_finder(item, args, "ap-password");
    let ap_dhcp = argument_finder(item, args, "ap-dhcp");
    let ap_leases = argument_finder(item, args, "ap-leases");
    let ap_burst = argument_finder(item, args, "ap-burst");
    let set_channel = argument_finder(item, args, "set-channel");
    let peer_mac = argument_finder(item, args, "peer-mac");
    let ht40 = argument_finder(item, args, "ht40");

    fn parse_on_off(s: &str) -> Option<bool> {
        match s {
            "on" | "true" | "1" | "yes" => Some(true),
            "off" | "false" | "0" | "no" => Some(false),
            _ => None,
        }
    }

    match mode {
        Ok(str) => {
            if str.is_some() {
                let token = str.unwrap();
                match NodeMode::from_token(token) {
                    Some(m) => USER_CONFIG.lock(|config| {
                        config.borrow_mut().as_mut().unwrap().node_mode = m;
                    }),
                    None => writeln!(
                        serial,
                        "Invalid WiFi Mode '{}' (use sniffer|station|wifi-ap|ht20-emitter|ht40-emitter|esp-now-initiator|esp-now-responder|esp-now-simplex-source|esp-now-simplex-peer)",
                        token
                    )
                    .unwrap(),
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
    match ap_ssid {
        Ok(str) => {
            if let Some(s) = str {
                let str_w_space = s.replace('\u{1F}', " ");
                let mut hpls_str_w_space = heapless::String::<32>::new();
                hpls_str_w_space.push_str(&str_w_space).unwrap();

                USER_CONFIG.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().ap_ssid =
                        hpls_str_w_space.try_into().unwrap();
                });
            }
        }
        Err(_) => (),
    }
    match ap_password {
        Ok(str) => {
            if let Some(s) = str {
                let str_w_space = s.replace('\u{1F}', " ");
                let mut hpls_str_w_space = heapless::String::<32>::new();
                hpls_str_w_space.push_str(&str_w_space).unwrap();

                USER_CONFIG.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().ap_password = hpls_str_w_space
                });
            }
        }
        Err(_) => (),
    }
    if let Ok(Some(s)) = ap_dhcp {
        match parse_on_off(s) {
            Some(v) => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().serve_dhcp = v;
            }),
            None => writeln!(serial, "Invalid --ap-dhcp (use on|off)").unwrap(),
        }
    }
    if let Ok(Some(s)) = ap_leases {
        match s.parse::<u8>() {
            Ok(n @ 1..=8) => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().ap_lease_count = n;
            }),
            _ => writeln!(serial, "Invalid --ap-leases (use 1-8)").unwrap(),
        }
    }
    if let Ok(Some(s)) = ap_burst {
        match parse_on_off(s) {
            Some(v) => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().ap_sync_burst = v;
            }),
            None => writeln!(serial, "Invalid --ap-burst (use on|off)").unwrap(),
        }
    }
    // One field, read differently per mode. Emitter modes: injection destination, empty
    // = broadcast. ESP-NOW modes: explicit peer, empty = automatic magic-prefix pairing;
    // setting it switches to source-MAC filtering, which needs BOTH nodes configured with
    // the other's address.
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
    // softAP secondary channel. `none` keeps the AP at HT20.
    if let Ok(Some(s)) = ht40 {
        match s {
            "above" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().ht40_secondary =
                    Some(SecondaryChannel::Above);
            }),
            "below" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().ht40_secondary =
                    Some(SecondaryChannel::Below);
            }),
            "none" | "off" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().ht40_secondary = None;
            }),
            _ => writeln!(serial, "Invalid --ht40 (use above|below|none)").unwrap(),
        }
    }
    // Emitter inter-frame period; the emitters' only rate control.
    //
    // Both flags write the SAME µs field, and the ms one is handled FIRST, so a host that sends
    // both in one `set-wifi` ends up with the µs value. That ordering is the compatibility story:
    // a host that only knows `--inject-period-ms` keeps its old behaviour exactly, and one that
    // knows both can send ms as a fallback for older firmware without it overriding the precise
    // value.
    if let Ok(Some(s)) = argument_finder(item, args, "inject-period-ms") {
        match s.parse::<u32>() {
            Ok(ms) if ms > 0 => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().inject_period_us = ms.saturating_mul(1000);
            }),
            _ => writeln!(
                serial,
                "Invalid --inject-period-ms (use a positive integer)"
            )
            .unwrap(),
        }
    }
    // The node's reporting — whether its measurements leave it. Only the modes that admit a
    // choice read it; on the ESP-NOW modes it also goes on the wire, so a responder paired with an
    // initiator that reports nothing can promote itself. Applied after `--mode`, so the sniffer
    // check sees the mode set in the same command.
    {
        let value = argument_finder(item, args, "reporting").ok().flatten();
        let report_to = argument_finder(item, args, "report-to").ok().flatten();
        if value.is_some() || report_to.is_some() {
            USER_CONFIG.lock(|config| {
                let mut cfg = config.borrow_mut();
                let cfg = cfg.as_mut().unwrap();
                apply_reporting_flags(serial, cfg, "reporting", value, report_to);
            });
        }
    }
    // HE20 (802.11ax SU) on the emitter modes and the ESP-NOW initiator/responder pair.
    #[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
    if let Ok(Some(v)) = argument_finder(item, args, "he20") {
        match parse_on_off(v) {
            Some(b) => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().he20 = b;
            }),
            None => writeln!(serial, "Invalid --he20 (use on|off)").unwrap(),
        }
    }
    if let Ok(Some(s)) = argument_finder(item, args, "inject-period-us") {
        match s.parse::<u32>() {
            Ok(us) if us > 0 => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().inject_period_us = us;
            }),
            _ => writeln!(
                serial,
                "Invalid --inject-period-us (use a positive integer)"
            )
            .unwrap(),
        }
    }

    // Which interface an emitter injects on. The driver accepts raw TX on both,
    // but only one may actually radiate on a given chip.
    if let Ok(Some(v)) = argument_finder(item, args, "emitter-iface") {
        match v.to_ascii_lowercase().as_str() {
            "sta" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().emitter_use_sta_if = true;
            }),
            "ap" => USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().emitter_use_sta_if = false;
            }),
            _ => writeln!(serial, "Invalid --emitter-iface '{}' (use sta|ap)", v).unwrap(),
        }
    }

    writeln!(serial, "\nUpdated WiFi Configuration:\n").unwrap();
    USER_CONFIG.lock(|config| {
        write_node_attributes(serial, config.borrow().as_ref().unwrap(), "");
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
        if cfg.ap_password.is_empty() {
            writeln!(
                serial,
                "Access Point Settings:\nSSID: '{}', Password: (open), DHCP: {}, Leases: {}, Burst: {}",
                cfg.ap_ssid, cfg.serve_dhcp, cfg.ap_lease_count, cfg.ap_sync_burst
            )
            .unwrap();
        } else {
            writeln!(
                serial,
                "Access Point Settings:\nSSID: '{}', Password: '{}', DHCP: {}, Leases: {}, Burst: {}",
                cfg.ap_ssid, cfg.ap_password, cfg.serve_dhcp, cfg.ap_lease_count, cfg.ap_sync_burst
            )
            .unwrap();
        }
        match cfg.peer_mac {
            Some(m) => writeln!(
                serial,
                "Peer MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} (emitter dst / ESP-NOW peer)",
                m[0], m[1], m[2], m[3], m[4], m[5]
            )
            .unwrap(),
            None => writeln!(
                serial,
                "Peer MAC: unset (emitter broadcasts / ESP-NOW auto-pairs)"
            )
            .unwrap(),
        }
        let ht40_str = match cfg.ht40_secondary {
            Some(SecondaryChannel::Above) => "HT40 (secondary above)",
            Some(SecondaryChannel::Below) => "HT40 (secondary below)",
            _ => "HT20/legacy",
        };
        writeln!(serial, "Secondary Channel: {}", ht40_str).unwrap();
        writeln!(serial, "Emitter Period: {}us", cfg.inject_period_us).unwrap();
        #[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
        writeln!(serial, "HE20: {}", cfg.he20).unwrap();
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

/// CLI command: `set-csi-output`
///
/// Toggles off-device delivery of captured CSI (`esp_csi_rs::set_csi_output_enabled`)
/// in [`USER_CONFIG`]. Disabling it leaves capture — and therefore the RX path and
/// its timing — untouched; nothing is decoded, logged, or handed to a callback.
///
/// # Options
/// - `--enabled=<true|false>` — Deliver captured CSI (default `true`).
///
/// Prints the updated setting after applying the change.
pub fn set_csi_output<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let mut touched = false;

    if let Ok(Some(s)) = argument_finder(item, args, "enabled") {
        match parse_on_off(s) {
            Some(v) => {
                USER_CONFIG.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().csi_output_enabled = v;
                });
                touched = true;
            }
            None => {
                writeln!(serial, "Invalid --enabled value. Use 'true' or 'false'.").unwrap();
                return;
            }
        }
    }

    if !touched {
        writeln!(
            serial,
            "Usage: set-csi-output --enabled=<true|false>"
        )
        .unwrap();
        return;
    }

    USER_CONFIG.lock(|config| {
        let cfg = config.borrow();
        let cfg = cfg.as_ref().unwrap();
        writeln!(serial, "\nCSI Output: {}", cfg.csi_output_enabled).unwrap();
    });
}

/// CLI command: `set-csi-filter`
///
/// Restricts which captured frames are delivered, by source MAC and/or PHY class.
///
/// A reporting receiver is promiscuous by nature — it reports CSI for every frame its radio decodes. On a
/// busy channel that means the AP's own beacons and ACKs, the association exchange, and any
/// third-party device on the channel, all mixed in with the traffic the user configured. Those rows
/// are real CSI, but they read as corrupt: the leading field is the frame's own 802.11 sequence
/// number (per-transmitter and per-TID, so arbitrary), and a legacy-rate frame yields the shorter
/// L-LTF-only payload.
///
/// Filtering here rather than on the host also buys back console bandwidth: the rejection happens in
/// the Wi-Fi callback before the packet copy and before any formatting.
pub fn set_csi_filter<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let mut touched = false;

    if let Ok(Some(s)) = argument_finder(item, args, "peer-mac") {
        let t = s.trim();
        if t.is_empty() || t.eq_ignore_ascii_case("any") || t.eq_ignore_ascii_case("off") {
            USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().csi_peer_filter = None;
            });
            esp_csi_rs::set_csi_peer_filter([0u8; 6]);
            touched = true;
        } else if let Some(mac) = parse_mac(t) {
            USER_CONFIG.lock(|config| {
                config.borrow_mut().as_mut().unwrap().csi_peer_filter = Some(mac);
            });
            esp_csi_rs::set_csi_peer_filter(mac);
            touched = true;
        } else {
            writeln!(
                serial,
                "Invalid --peer-mac value. Use aa:bb:cc:dd:ee:ff, or 'any' to clear."
            )
            .unwrap();
            return;
        }
    }

    if let Ok(Some(s)) = argument_finder(item, args, "min-phy") {
        let t = s.trim();
        let min = if t.eq_ignore_ascii_case("any") {
            Some(0u8)
        } else if t.eq_ignore_ascii_case("ht") {
            Some(1u8)
        } else {
            None
        };
        match min {
            Some(v) => {
                USER_CONFIG.lock(|config| {
                    config.borrow_mut().as_mut().unwrap().csi_min_sig_mode = v;
                });
                esp_csi_rs::set_csi_min_sig_mode(v);
                touched = true;
            }
            None => {
                writeln!(serial, "Invalid --min-phy value. Use 'any' or 'ht'.").unwrap();
                return;
            }
        }
    }

    if !touched {
        writeln!(
            serial,
            "Usage: set-csi-filter [--peer-mac=<aa:bb:cc:dd:ee:ff|any>] [--min-phy=<any|ht>]"
        )
        .unwrap();
        return;
    }

    USER_CONFIG.lock(|config| {
        let cfg = config.borrow();
        let cfg = cfg.as_ref().unwrap();
        match cfg.csi_peer_filter {
            Some(m) => writeln!(
                serial,
                "\nCSI Peer Filter: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                m[0], m[1], m[2], m[3], m[4], m[5]
            )
            .unwrap(),
            None => writeln!(serial, "\nCSI Peer Filter: any source").unwrap(),
        }
        writeln!(
            serial,
            "CSI Min PHY: {}",
            if cfg.csi_min_sig_mode == 0 {
                "any"
            } else {
                "ht (802.11n and better)"
            }
        )
        .unwrap();
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
/// grouped into four sections: `[Node]`, `[WiFi]`, `[Measurement]`, and `[CSI Config]`.
///
/// CSI Config fields are platform-specific: ESP32-C5/C6 expose per-PPDU-format
/// acquisition flags while all other targets expose classic LLTF/HTLTF fields.
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

        // Node attributes (IEEE 802.11bf terms), read back from the Mode the run is built with.
        writeln!(serial, "[Node]").unwrap();
        write_node_attributes(serial, cfg, "  ");
        writeln!(serial, "  Periodicity: {} Hz", cfg.periodicity_hz).unwrap();

        // WiFi settings
        writeln!(serial, "\n[WiFi]").unwrap();
        writeln!(serial, "  Channel : {}", cfg.channel).unwrap();
        if matches!(cfg.node_mode, NodeMode::WifiStation) {
            #[cfg(feature = "esp32c5")]
            writeln!(
                serial,
                "  STA band  : channel {} (C5 dual-band hint)",
                cfg.channel
            )
            .unwrap();
        }
        writeln!(serial, "  STA SSID: '{}'", cfg.sta_ssid).unwrap();
        writeln!(serial, "  STA Pass: '{}'", cfg.sta_password).unwrap();
        writeln!(serial, "  AP SSID : '{}'", cfg.ap_ssid).unwrap();
        if cfg.ap_password.is_empty() {
            writeln!(serial, "  AP Pass : open").unwrap();
        } else {
            writeln!(serial, "  AP Pass : '{}'", cfg.ap_password).unwrap();
        }
        writeln!(serial, "  AP DHCP : {}", cfg.serve_dhcp).unwrap();
        writeln!(serial, "  AP Leases: {}", cfg.ap_lease_count).unwrap();
        writeln!(serial, "  AP Burst : {}", cfg.ap_sync_burst).unwrap();
        match cfg.peer_mac {
            Some(m) => writeln!(
                serial,
                "  Dst MAC : {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                m[0], m[1], m[2], m[3], m[4], m[5]
            )
            .unwrap(),
            None => writeln!(serial, "  Dst MAC : broadcast").unwrap(),
        }
        let ht40_str = match cfg.ht40_secondary {
            Some(SecondaryChannel::Above) => "HT40 (secondary above)",
            Some(SecondaryChannel::Below) => "HT40 (secondary below)",
            _ => "HT20/legacy",
        };
        writeln!(serial, "  AP 2nd  : {}", ht40_str).unwrap();
        #[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
        writeln!(serial, "  HE20    : {}", cfg.he20).unwrap();

        // Measurement / reporting settings
        writeln!(serial, "\n[Measurement]").unwrap();
        writeln!(
            serial,
            "  Report to     : {}",
            match cfg.report_to {
                ReportTo::Local => "local",
                _ => "host",
            }
        )
        .unwrap();
        writeln!(
            serial,
            "  Threshold     : level={}, hold={}ms; Decimate n={}",
            cfg.threshold.level, cfg.threshold.hold_ms, cfg.decimate_n
        )
        .unwrap();
        writeln!(serial, "  Session ID    : {}", esp_csi_rs::session_id()).unwrap();
        writeln!(serial, "  CSI Output    : {}", cfg.csi_output_enabled).unwrap();
        match cfg.csi_peer_filter {
            Some(m) => writeln!(
                serial,
                "  CSI Peer Filt : {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                m[0], m[1], m[2], m[3], m[4], m[5]
            )
            .unwrap(),
            None => writeln!(serial, "  CSI Peer Filt : any source").unwrap(),
        }
        writeln!(
            serial,
            "  CSI Min PHY   : {}",
            if cfg.csi_min_sig_mode == 0 { "any" } else { "ht" }
        )
        .unwrap();
        writeln!(serial, "  PHY Rate      : {:?}", cfg.phy_rate).unwrap();
        writeln!(serial, "  Protocol      : {:?}", cfg.protocol).unwrap();
        writeln!(
            serial,
            "  IO Tasks      : tx={}, rx={}",
            cfg.io_tasks.tx_enabled, cfg.io_tasks.rx_enabled
        )
        .unwrap();
        writeln!(
            serial,
            "  Emitter       : period={}us",
            cfg.inject_period_us
        )
        .unwrap();

        // CSI configuration (platform-specific fields)
        writeln!(serial, "\n[CSI Config]").unwrap();
        #[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
        {
            writeln!(serial, "  Acquire CSI        : {}", cfg.csi_config.enable).unwrap();
            writeln!(
                serial,
                "  Legacy (11g)       : {}",
                cfg.csi_config.acquire_csi_legacy
            )
            .unwrap();
            writeln!(
                serial,
                "  HT20               : {}",
                cfg.csi_config.acquire_csi_ht20
            )
            .unwrap();
            writeln!(
                serial,
                "  HT40               : {}",
                cfg.csi_config.acquire_csi_ht40
            )
            .unwrap();
            writeln!(
                serial,
                "  Scale Value        : {}",
                cfg.csi_config.val_scale_cfg
            )
            .unwrap();
            writeln!(
                serial,
                "  Dump ACK           : {}",
                cfg.csi_config.dump_ack_en
            )
            .unwrap();
            writeln!(
                serial,
                "  HE-LTF SU/MU/DCM   : {}/{}/{}",
                cfg.csi_config.acquire_csi_su,
                cfg.csi_config.acquire_csi_mu,
                cfg.csi_config.acquire_csi_dcm
            )
            .unwrap();
            writeln!(
                serial,
                "  HE-LTF BF/HE-STBC  : {}/{}",
                cfg.csi_config.acquire_csi_beamformed,
                cfg.csi_config.acquire_csi_he_stbc
            )
            .unwrap();
            #[cfg(feature = "esp32c5")]
            {
                writeln!(
                    serial,
                    "  Force L-LTF        : {}",
                    cfg.csi_config.acquire_csi_force_lltf
                )
                .unwrap();
                writeln!(
                    serial,
                    "  VHT20              : {}",
                    cfg.csi_config.acquire_csi_vht
                )
                .unwrap();
            }
        }
        #[cfg(not(any(feature = "esp32c5", feature = "esp32c6")))]
        {
            writeln!(serial, "  LLTF Enabled       : {}", cfg.csi_config.lltf_en).unwrap();
            writeln!(serial, "  HTLTF Enabled      : {}", cfg.csi_config.htltf_en).unwrap();
            writeln!(
                serial,
                "  STBC HTLTF Enabled : {}",
                cfg.csi_config.stbc_htltf2_en
            )
            .unwrap();
            writeln!(
                serial,
                "  LTF Merge Enabled  : {}",
                cfg.csi_config.ltf_merge_en
            )
            .unwrap();
            writeln!(
                serial,
                "  Channel Filter     : {}",
                cfg.csi_config.channel_filter_en
            )
            .unwrap();
            writeln!(
                serial,
                "  Manual Scale       : {}",
                cfg.csi_config.manu_scale
            )
            .unwrap();
            writeln!(serial, "  Shift Bits         : {}", cfg.csi_config.shift).unwrap();
            writeln!(
                serial,
                "  Dump ACK           : {}",
                cfg.csi_config.dump_ack_en
            )
            .unwrap();
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
/// Records the Wi-Fi PHY rate in [`USER_CONFIG`] for `show-config`. No mode
/// applies it any more: receivers derive their rate from the surrounding radio
/// configuration, and an emitter transmits at the rate its forced TX PHY implies. The
/// ESP-NOW initiator / responder pair DOES apply it, as the per-peer TX PHY.
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
/// node via `CsiNode::set_protocol` at the start of each collection run.
///
/// # Options
/// - `--protocol=<NAME>` — One of: `b`, `g`, `n`, `lr`, `a`, `ac`, and `ax` on the ESP32-C5/C6
///   (802.11ax; esp-csi-rs runs the HE20 bring-up for station, wifi-ap and sniffer).
///
/// `lr` (Espressif long-range) is the default and suits sniffer links between ESP
/// devices; use `n` when associating to a standard AP in station mode.
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
            #[cfg(any(feature = "esp32c5", feature = "esp32c6"))]
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
                "Invalid protocol. Use one of: b, g, n, lr (default), a, ac, ax (C5/C6)."
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
///   packets out. `async` queues to `CsiNodeClient::next_csi_packet`,
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
                writeln!(
                    serial,
                    "Invalid mode. Use 'off', 'callback', 'async', or 'raw'."
                )
                .unwrap();
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
/// log=<text|defmt>
/// transport=<uart|jtag|auto>
/// baud=<u32>
/// features=<comma-separated-list>
/// END-INFO
/// ```
///
/// The `mac` field (protocol >= 2) is the factory eFuse MAC, which is also the
/// USB `iSerialNumber` on native-USB boards. Host tooling uses it as the stable
/// device key so a `restart`/re-enumeration re-binds to the same board.
///
/// ## `baud` is additive, and the protocol version deliberately does NOT move
///
/// The console rate is fixed at build time (see `build.rs`), and a host that opens the port at the
/// wrong rate reads garbage that looks like a hardware fault rather than a settings mismatch.
/// Declaring it lets the host confirm the rate from the device's own answer instead of assuming.
///
/// Adding the line is not a grammar change: the format is "key=value lines up to `END-INFO`", and a
/// host parser ends its match with `_ => {}`. Bumping [`CLI_PROTOCOL_VERSION`] for it would be
/// actively harmful — a host checks the protocol with an exact equality against a constant compiled
/// into its own binary, so a board reflashed with a higher number would be refused at attach as
/// needing a reflash. Reserve the bump for a change that actually breaks the grammar. Absent means
/// `115200`, which is what every build before this one used.
///
/// Note the bootstrap limit: `info` can only CONFIRM the baud once a host is already talking at it,
/// so the release manifest carries the same value per asset for tooling that must pick a rate before
/// the first byte.
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
    // Read from the build — nothing at runtime can move it.
    // What the log frames ARE. Read from the build, not from a runtime setting, because it is the
    // build that decides: `defmt` interns its format strings at compile time.
    //
    // This line exists because a defmt build emits BINARY frames on the same serial line a host
    // parses as CSI text, and the host has no other way to tell — the bytes are not
    // self-describing, and a defmt stream fed to a text parser produces garbage rows that look
    // like a hardware fault rather than a decoder mismatch. Declaring the encoding here lets the
    // host pick its decoder from the device's own answer.
    let log = if cfg!(feature = "defmt") { "defmt" } else { "text" };
    // Which wire they leave on. `auto` is a real answer, not a missing one: the board wires both
    // consoles and writes to whichever has a host attached, so naming one would be a guess the
    // host might act on.
    let transport = if cfg!(feature = "jtag-serial") {
        "jtag"
    } else if cfg!(feature = "uart") {
        "uart"
    } else if cfg!(feature = "esp32") {
        // No USB-Serial-JTAG peripheral on this part: there is exactly one wire, and `auto` would
        // be a lie.
        "uart"
    } else {
        "auto"
    };
    writeln!(serial, "log={}", log).unwrap();
    writeln!(serial, "transport={}", transport).unwrap();
    writeln!(serial, "baud={}", crate::UART_BAUD).unwrap();
    writeln!(serial, "features={}", features).unwrap();
    writeln!(serial, "END-INFO").unwrap();
}

/// CLI command: `version`
///
/// Prints `open <semver>` — the build flavor followed by the crate version.
///
/// Host tooling reads the flavor from here rather than probing a
/// proprietary-only mode string and inspecting the rejection: a positive
/// statement survives mode renames, and the proprietary build answers
/// `pro <semver>` from the same command.
pub fn version_cmd<'a>(
    _menu: &Menu<SerialInterface, Context>,
    _item: &Item<SerialInterface, Context>,
    _args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    writeln!(serial, "open {}", env!("CARGO_PKG_VERSION")).unwrap();
}

/// CLI command: `restart`
///
/// Requests a software reset of the chip. The reset is NOT performed inline:
/// resetting with the radio live has been observed to hang the next boot on
/// ESP32-C5 (single ROM banner, application never starts, only the EN button
/// recovers). Instead this signals the collection task — which owns the WiFi
/// controller — to stop any active run, deinit the radio (esp-radio
/// `wifi_deinit` via the controller's destructor), and only then call
/// [`esp_hal::system::software_reset`].
///
/// On boards whose serial transport is the built-in USB-Serial-JTAG
/// peripheral, the reset drops and re-enumerates the USB device. Because host
/// tooling pins to the board's MAC (the `mac=` field of `info`) rather than
/// the `/dev/ttyACM*` path, the re-enumeration is a non-event — the
/// per-device task re-binds to the same physical board on the new node
/// number, and the reboot re-emits the welcome banner the host greps to
/// confirm the board is back.
pub fn restart_cmd<'a>(
    _menu: &Menu<SerialInterface, Context>,
    _item: &Item<SerialInterface, Context>,
    _args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    writeln!(serial, "\nRestarting (radio deinit first)...").unwrap();
    // Push the message out of the peripheral FIFO before the reset tears the
    // transport down, otherwise it can be lost on the way out.
    let _ = Write::flush(serial);
    crate::config::RESTART_PENDING.store(true, Ordering::Relaxed);
    crate::config::RESTART_SIGNAL.signal(());
    if crate::config::IS_COLLECTING.load(Ordering::Relaxed) {
        // Unwind the active run first; the collection task performs the
        // radio deinit + reset right after the run tears down.
        crate::config::STOP_REQUEST.signal(());
    }
}

/// CLI command: `show-stats` (compiled in only with the `statistics` feature)
///
/// Reads the runtime CSI/traffic counters exposed by `esp-csi-rs` and prints
/// a one-shot snapshot. These counters are accumulated by the WiFi callback and
/// the traffic-generation path; they are most meaningful while a collection is
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
    // Losses (oversize, queue full, sequence gaps) sum to `RX Dropped Pkts`. Filtered and
    // policy-suppressed frames are deliberate and no longer counted as drops (esp-csi-rs 0.12).
    let d = get_drop_breakdown();
    writeln!(serial, "    oversize       : {}", d.oversize).unwrap();
    writeln!(serial, "    queue full     : {}", d.queue_full).unwrap();
    writeln!(serial, "    sequence gaps  : {}", d.seq_gap).unwrap();
    writeln!(serial, "  RX Filtered      : {}", d.filtered).unwrap();
    writeln!(serial, "  RX Policy Supp.  : {}", d.policy_suppressed).unwrap();
    writeln!(serial, "  Log Dropped      : {}", d.log_dropped).unwrap();
    writeln!(serial, "================================\n").unwrap();
}

/// CLI command: `set-reporting`
///
/// Sets the node's reporting and its parameters. A superset of
/// `set-wifi --reporting=`, which picks the value but not its parameters.
///
/// # Options
/// - `--policy=<report|none|threshold|decimate>` — the reporting (a sniffer refuses `none`).
/// - `--report-to=<host|local>` — where `report` sends measurements (default host).
/// - `--level=<0-65535>` — threshold score at or above which a measurement is reported.
/// - `--hold-ms=<ms>` — how long to keep reporting after a measurement crosses `level`.
/// - `--n=<N>` — decimation: report every N-th measurement.
///
/// Parameters are stored even when another value is active, and take effect immediately when the
/// matching value already is.
pub fn set_reporting_cmd<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    USER_CONFIG.lock(|config| {
        let mut cfg = config.borrow_mut();
        let cfg = cfg.as_mut().unwrap();
        if let Ok(Some(v)) = argument_finder(item, args, "level") {
            match v.parse::<u16>() {
                Ok(l) => cfg.threshold.level = l,
                Err(_) => writeln!(serial, "Invalid --level (use 0-65535)").unwrap(),
            }
        }
        if let Ok(Some(v)) = argument_finder(item, args, "hold-ms") {
            match v.parse::<u16>() {
                Ok(h) => cfg.threshold.hold_ms = h,
                Err(_) => writeln!(serial, "Invalid --hold-ms (use 0-65535)").unwrap(),
            }
        }
        if let Ok(Some(v)) = argument_finder(item, args, "n") {
            match v.parse::<u16>() {
                Ok(n) if n > 0 => cfg.decimate_n = n,
                _ => writeln!(serial, "Invalid --n (use 1-65535)").unwrap(),
            }
        }
        // Re-derive an active parameterised value so new parameters apply without restating it.
        match cfg.reporting {
            Reporting::ThresholdBased(_) => {
                cfg.reporting = Reporting::ThresholdBased(cfg.threshold)
            }
            Reporting::Decimate(_) => cfg.reporting = Reporting::Decimate(cfg.decimate_n),
            _ => {}
        }
        let value = argument_finder(item, args, "policy").ok().flatten();
        let report_to = argument_finder(item, args, "report-to").ok().flatten();
        apply_reporting_flags(serial, cfg, "policy", value, report_to);
        writeln!(serial, "\nReporting: {}", reporting_line(cfg)).unwrap();
    });
}

/// CLI command: `set-session`
///
/// Names the measurement session (`esp_csi_rs::set_session`). The id is stamped on every
/// serialized frame's envelope from now on, across runs, until changed or the board resets.
///
/// # Options
/// - `--id=<u32>` — session id, decimal or `0x`-prefixed hex. Required.
/// - `--epoch=<unix_us>` — the current UNIX time in microseconds. Anchors the session to wall
///   time: the next run announces it in its `SessionInfo` frame. Omit to announce no wall time.
pub fn set_session_cmd<'a>(
    _menu: &Menu<SerialInterface, Context>,
    item: &Item<SerialInterface, Context>,
    args: &[&str],
    serial: &mut SerialInterface,
    _context: &mut Context,
) {
    let id = match argument_finder(item, args, "id") {
        Ok(Some(v)) => {
            let parsed = match v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => v.parse::<u32>().ok(),
            };
            match parsed {
                Some(id) => id,
                None => {
                    writeln!(serial, "Invalid --id (use a u32, decimal or 0x hex)").unwrap();
                    return;
                }
            }
        }
        _ => {
            writeln!(serial, "Usage: set-session --id=<u32> [--epoch=<unix_us>]").unwrap();
            return;
        }
    };
    let epoch = match argument_finder(item, args, "epoch") {
        Ok(Some(v)) => match v.parse::<u64>() {
            Ok(us) => Some(us),
            Err(_) => {
                writeln!(serial, "Invalid --epoch (use UNIX time in microseconds)").unwrap();
                return;
            }
        },
        _ => None,
    };
    esp_csi_rs::set_session(id, epoch);
    match epoch {
        Some(us) => writeln!(serial, "\nSession: id={}, epoch={}us", id, us).unwrap(),
        None => writeln!(serial, "\nSession: id={}, no wall-clock anchor", id).unwrap(),
    }
}
