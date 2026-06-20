fn main() {
    // Warn when `defmt` and `async-print` are enabled together. The two cannot
    // be combined: esp-csi-rs's `defmt` feature always pulls in esp-println's
    // `defmt-espflash` global logger, while its async-print path registers a
    // second `#[defmt::global_logger]` — so the build fails to link with
    // `_defmt_acquire` multiply defined. Because `jtag-serial` forces
    // `async-print`, this also rules out `defmt` + `jtag-serial`.
    //
    // Surfaced here as a readable warning ahead of the otherwise cryptic
    // linker error. For the fastest non-blocking collection, use
    // `jtag-serial` (async-print on) with the `serialized` log mode instead of
    // `defmt`.
    if std::env::var_os("CARGO_FEATURE_DEFMT").is_some()
        && std::env::var_os("CARGO_FEATURE_ASYNC_PRINT").is_some()
    {
        println!(
            "cargo:warning=`defmt` + `async-print` are not supported together \
             (duplicate defmt global_logger -> `_defmt_acquire` multiply defined; \
             this also rules out `defmt` + `jtag-serial`, which forces async-print). \
             For the most optimal non-blocking collection setup, build with \
             `jtag-serial` (enables async-print) and select the `serialized` log \
             mode at runtime (`set-log-mode --mode=serialized`)."
        );
    }
}
