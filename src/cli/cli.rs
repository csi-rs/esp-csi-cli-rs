use core::fmt::Write;
use menu::Menu;
use crate::cli::{SerialInterface, Context};

/// Called by the `menu` crate whenever the CLI runner enters (or re-enters) the root menu.
///
/// Prints the welcome banner and a summary of available commands to the serial interface.
pub fn enter_root(
    _menu: &Menu<SerialInterface, Context>,
    interface: &mut SerialInterface,
    _context: &mut Context,
) {
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
    start                   Start the CSI collection process with a defined duration.
    show-config             Display the current configuration settings.
    reset-config            Reset all configurations to their default values.
    help                    Display this help menu or details for a specific command.

    For more information on a specific command, type:
    help <command>

    Example:
    help set-traffic",
        )
        .unwrap();
    // interface.flush().unwrap();
}