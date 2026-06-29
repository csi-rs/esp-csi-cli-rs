use core::fmt::Write;
use menu::Menu;
use crate::cli::cmds::device_mac;
use crate::cli::{SerialInterface, Context};

/// Called by the `menu` crate whenever the CLI runner enters (or re-enters) the root menu.
///
/// Prints the welcome banner and a summary of available commands to the serial interface.
pub fn enter_root(
    _menu: &Menu<SerialInterface, Context>,
    interface: &mut SerialInterface,
    _context: &mut Context,
) {
    // Magic identification line. Host-side tooling greps for this prefix to
    // recognize esp-csi-cli-rs firmware on reset. Keep this in sync with the
    // first line emitted by the `info` command in cmds.rs.
    interface
        .write_str(concat!("ESP-CSI-CLI/", env!("CARGO_PKG_VERSION"), "\n"))
        .unwrap();
    // Stable per-board identifier (factory eFuse MAC = USB iSerialNumber on
    // native-USB boards). Emitted on every reset so host tooling can pin the
    // device by serial number and treat re-enumeration as a non-event. Kept in
    // sync with the `mac=` field of the `info` command in cmds.rs.
    let mac = device_mac();
    writeln!(
        interface,
        "mac={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
    .unwrap();
    interface
        .write_str("******* Welcome to the CSI Collection CLI utility! *******")
        .unwrap();
    interface.write_str("\n").unwrap();
    interface
        .write_str(
            "Available Commands:
    set-wifi                Configure WiFi settings (e.g., mode).
    set-traffic             Configure traffic-related parameters (e.g. interval).
    set-collection-mode     Set the CSI node collection mode (collector or listener).
    set-log-mode            Set the CSI output logging format (text, array-list, serialized).
    set-csi                 Configure CSI feature flags (e.g., LLTF, HTLTF).
    set-rate                Pin the Wi-Fi PHY rate (ESP-NOW modes only).
    set-io-tasks            Toggle TX and/or RX direction tasks.
    set-csi-delivery        Switch CSI delivery mode and inline log gate.
    start                   Start the CSI collection process with a defined duration.
    show-config             Display the current configuration settings.
    show-stats              Print runtime CSI / traffic counters (statistics feature).
    reset-config            Reset all configurations to their default values.
    restart                 Reboot the device via a clean software reset.
    info                    Print firmware identification metadata.
    help                    Display this help menu or details for a specific command.

    For more information on a specific command, type:
    help <command>

    Example:
    help set-traffic",
        )
        .unwrap();
    // interface.flush().unwrap();
}