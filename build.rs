/// Name of the environment variable that selects the console baud rate.
///
/// Deliberately a BUILD-TIME setting rather than a `set-uart --baud=` CLI command. A runtime knob
/// would have to change the baud of the very console carrying the command: the reply is either lost
/// or sent at the old rate while the host has already switched, and surviving a reset would then
/// need either a persisted setting or a re-negotiation handshake. It would also make a board's wire
/// rate depend on invisible prior state. The rate is instead immutable for a given image, and
/// DECLARED — `info` reports it as `baud=`, and the release manifest carries it per asset so host
/// flash tooling knows the rate before it ever opens the port.
const BAUD_ENV: &str = "ESP_CSI_CLI_UART_BAUD";

/// The console baud rate every build before this one used, and therefore the default.
const DEFAULT_BAUD: u32 = 115_200;

fn emit_uart_baud() {
    // Without this, changing the variable would not rebuild and the image would silently keep the
    // previously compiled rate — the worst possible failure for a setting whose whole job is to
    // match what the host opens the port at.
    println!("cargo:rerun-if-env-changed={BAUD_ENV}");

    let baud = match std::env::var(BAUD_ENV) {
        Ok(raw) => {
            let trimmed = raw.trim();
            match trimmed.parse::<u32>() {
                // esp-hal's `uart::Config` validator rejects 0 and anything above 5 MBd; catch it
                // here so the failure names the variable instead of surfacing as a runtime
                // `Uart::new` unwrap panic on a board that then looks bricked.
                Ok(v) if (1..=5_000_000).contains(&v) => v,
                _ => panic!(
                    "{BAUD_ENV}={trimmed:?} is not a valid baud rate \
                     (expected an integer in 1..=5000000, e.g. 921600)"
                ),
            }
        }
        Err(_) => DEFAULT_BAUD,
    };

    // A generated `const` rather than `cargo::rustc-env` + `env!`, because the consumer needs a
    // `u32` at compile time and parsing a `&str` in a const context is needless ceremony.
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("uart_baud.rs");
    std::fs::write(
        &out,
        format!(
            "/// Console baud rate, fixed at build time by `{BAUD_ENV}` (default {DEFAULT_BAUD}).\n\
             pub const UART_BAUD: u32 = {baud};\n"
        ),
    )
    .unwrap();

    if baud != DEFAULT_BAUD {
        // Loud on purpose: a board built at a non-default rate is unreadable to a host that opens
        // the port at 115200, and the boot ROM banner stays at 115200 regardless.
        println!(
            "cargo:warning=console baud is {baud} (not the {DEFAULT_BAUD} default) — open the \
             serial port at {baud}; the ROM bootloader banner is still emitted at {DEFAULT_BAUD}."
        );
    }
}

fn main() {
    emit_uart_baud();

    // `defmt` needs the `-Tdefmt.x` linker script. Cargo does not propagate linker arguments from
    // a dependency's build script, so `esp-csi-rs` adding it for itself does nothing for this
    // binary — the flag has to come from this crate's own build, and the `*-defmt` cargo aliases
    // supply it via `--config target.<triple>.rustflags`.
    //
    // Without it the link fails on `undefined symbol: _defmt_panic` / `_defmt_timestamp`, which
    // reads like missing application code rather than a missing linker script. This warning names
    // the actual cause.
    //
    // This block previously warned that `defmt` and `async-print` could not be combined, claiming
    // two `#[defmt::global_logger]`s and `_defmt_acquire` multiply defined, and that this ruled out
    // `defmt` + `jtag-serial`. None of that is true: there is one global logger, in `esp-csi-rs`,
    // and every transport links with `defmt`. Verified on the ESP32-C6 for `auto`, `uart` and
    // `jtag-serial`. The warning was emitted on builds that then succeeded.
    if std::env::var_os("CARGO_FEATURE_DEFMT").is_some() {
        println!(
            "cargo:warning=`defmt` requires the `-Tdefmt.x` linker script. Build with a `*-defmt` \
             cargo alias (e.g. `cargo esp32c6-defmt`), which adds it; a plain `cargo build \
             --features=defmt` will fail to link with undefined `_defmt_panic` / `_defmt_timestamp`."
        );
    }
}
