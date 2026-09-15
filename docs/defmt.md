# Logging with `defmt`

## Enabling Logging w/ `defmt`

This application can use either the standard `println!` macros or the `defmt` framework for logging. `defmt` produces compact binary frames that the host (`espflash`, `probe-rs`, etc.) decodes against the original ELF, so it's both faster on the device and richer on the host.

The recommended way is the `*-defmt` / `*-defmt-build` cargo aliases — pick the one matching your chip:

```bash
cargo esp32c6-defmt          # build + flash + monitor with defmt decoding
cargo esp32c6-defmt-build    # build only, skip flashing
```

Each defmt alias automatically:
- drops `println` from the default features and enables `defmt`,
- appends `-Tdefmt.x` to the linker script set (so the `.defmt` ELF section is emitted),
- swaps `espflash`'s runner to `espflash flash --monitor --log-format defmt` so log frames are decoded inline.

No edits to `.cargo/config.toml` are required — the aliases pass everything through `cargo --config` overrides at invocation time. If you want to invoke `cargo build` directly (e.g. in CI), the equivalent is:

```bash
cargo build --release \
  --no-default-features \
  --features esp32c6,defmt,no-std,auto,statistics \
  --target riscv32imac-unknown-none-elf \
  --config 'target.riscv32imac-unknown-none-elf.rustflags=["-C", "link-arg=-Tdefmt.x"]'
```

Xtensa targets (`esp32`, `esp32s3`) need the `-Wl,` prefix on the link arg because their toolchain goes through a GCC linker driver:

```bash
--config 'target.xtensa-esp32s3-none-elf.rustflags=["-C", "link-arg=-Wl,-Tdefmt.x"]'
```
