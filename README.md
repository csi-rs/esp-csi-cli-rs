# esp-csi-cli-rs

`esp-csi-cli-rs` is a command-line interface (CLI) application that runs on top of the `esp-csi-rs` crate.  `esp-csi-cli-rs` provides a user friendly interface for configuring and collecting Wi-Fi Channel State Information (CSI) on ESP devices. It allows users to configure various parameters related to CSI data collection.

In order to use this crate, you would need to flash the source code for your target device. Currently supported devices include:

- ESP32
- ESP32-C3
- ESP32-C6
- ESP32-S3

<div align="center">

![CLI Snapshot](/assets/cli_snapshot.png)

</div>

## Features

* **Multiple Wi-Fi Modes:** Configure the ESP device as an Access Point (AP), Station (STA), AP+STA, or Sniffer.
* **Traffic Generation:** Generate traffic at configurable intervals.
* **Fine-grained CSI Control:** Enable or disable specific CSI features like LLTF, HTLTF, STBC HTLTF, and LTF Merge.
* **CLI Control:** Interact with the device using simple commands over a serial connection.
* **Configuration Management:** Show the current configuration or reset to defaults.
* **Timed Collection:** Start CSI collection for a specific duration or run indefinitely.
* **Flexible Logging:** Supports standard `println!` or the more efficient `defmt` logging.


## Minimum Requirements

* **Hardware:** An ESP development board (e.g., ESP32-C3, ESP32-S3, ESP32...etc.).
* **Software:** A tool to flash binaries and a tool to monitor output. It is recommended to use `espflash` as it supports both. Additionally, `espflash` supports `defmt` log interpretation. Installation instructions are available [here](https://docs.esp-rs.org/book/tooling/espflash.html). 

> ‼️ Installing `espflash` requires a Rust installation. If you don't have Rust installed, follow the instructions on the [rustup](https://rustup.rs/) website.

## Usage

1.  **Download Binary:** Navigate to the /binaries folder in the repository and identify the correct binary .elf file for your ESP chip. There are binaries supporting both regular `println` logging and the more efficient `defmt` loggging. Here are the file names for the different devices:

<div align="center">

| Target Board           | Binary Filename                              |
|------------------------|----------------------------------------------|
| ESP32-C3               | `esp-csi-cli-rs-esp32c3.elf`                 |
| ESP32-C3 (defmt)       | `esp-csi-cli-rs-esp32c3-defmt.elf`           |
| ESP32-C6               | `esp-csi-cli-rs-esp32c6.elf`                 |
| ESP32-C6 (defmt)       | `esp-csi-cli-rs-esp32c6-defmt.elf`           |
| ESP32-S3               | `esp-csi-cli-rs-esp32s3.elf`                 |
| ESP32-S3 (defmt)       | `esp-csi-cli-rs-esp32s3-defmt.elf`           |
| ESP32                  | `esp-csi-cli-rs-esp32.elf`                   |
| ESP32 (defmt)          | `esp-csi-cli-rs-esp32-defmt.elf`             |

</div>

> 📝 Using `defmt` binaries requires that you use a serial monitoring tool capable of interpreting `defmt` encoding such as `espflash`. If you do not, typically you would observe weird characters appear on the monitoring output.

2.  **Flash:** Connect to your ESP device over USB and use `espflash` to flash the downloaded binary to your ESP by running the following command:
    ```bash
    espflash flash [path to downloaded .elf binary]
    ```
3.  **Monitor & Interact with CLI:** Use `espflash` to connect to and interact with your device by running either of the commands below. If you are using a file with a `defmt` extension note that you'll need to pass the same .elf file from step 2 to the monitor.
    ```bash
    # Example w/o defmt
    espflash --monitor

    # Example w/ defmt
    espflash --monitor -—elf [path to attached file] -—log-format defmt 
    ```
> 📝 When logging over `defmt` the monitor requires the original binary to be able to decode the incoming characters.

> 🛑 Step 2 needs only to be performed once. If you disconnect the ESP device all you need to do is run step 3 when reconnecting it followed by ctrl+R to reset the device. 

> 🛑 If you encounter strange behaviour with the CLI, it often helps to press ctrl+R to reset the device. Alternatively, you can terminate the whole session by pressing ctrl+C. Session termination requires that you run step 3 again to activate the monitor.

## CLI Commands

This is a list of commands available through the CLI interface:
> 📝 The `set-csi` commands for the ESP32-C6 will be different.

* **`help [command]`**
    * Description: Display the main help menu or details for a specific command.
    * Example: `help set-wifi`

* **`set-traffic [OPTIONS]`**
    * Description: Configure traffic-related parameters.
    * Options:
        * `--frequency-hz=<NUMBER>`: Specify the traffic frequency in Hertz (default: 0).
    * Examples:
        * `set-traffic --frequency-hz=10`

* **`set-csi [OPTIONS]`**
    * Description: Configure CSI feature flags.
    * Options:
        * `--disable-lltf`: Disable LLTF CSI (default: enabled).
        * `--disable-htltf`: Disable HTLTF CSI (default: enabled).
        * `--disable-stbc-htltf`: Disable STBC HTLTF CSI (default: enabled).
        * `--disable-ltf-merge`: Disable LTF Merge CSI (default: enabled).
    * Examples:
        * `set-csi --disable-lltf --disable-ltf-merge`
        * `set-csi --disable-htltf`

* **`set-wifi [OPTIONS]`**
    * Description: Configure WiFi settings. **Note:** Replace spaces in SSIDs or Passwords with underscores (`_`).
    * Options:
        * `--mode=<ap|station|sniffer|ap-station>`: Specify WiFi operation mode (default: sniffer).
        * `--max-connections=<NUMBER>`: Set the maximum number of AP connections (default: 1).
        * `--hide-ssid`: Hide the SSID for the AP (default: visible).
        * `--ap-ssid=<SSID>`: Set the SSID for the AP.
        * `--ap-password=<PASSWORD>`: Set the password for the AP.
        * `--sta-ssid=<SSID>`: Set the SSID for the station.
        * `--sta-password=<PASSWORD>`: Set the password for the station.
    * Examples:
        * `set-wifi --mode ap --max-connections 5 --hide-ssid --ap-ssid=My_ESP_AP --ap-password=secret_pass`
        * `set-wifi --mode station --sta-ssid=My_Home_Network --sta-password=my_wifi_key`

* **`start [OPTIONS]`**
    * Description: Start the CSI collection process. Ensure the device is configured first.
    * Options:
        * `--duration=<SECONDS>`: Specify the duration (in seconds) for CSI collection. If omitted, collection runs indefinitely (or technically, for about a week).
    * Examples:
        * `start`
        * `start --duration=120`

* **`show-config`**
    * Description: Display the current configuration settings for all parameters.
    * Example: `show-config`

* **`reset-config`**
    * Description: Reset all configurations to their default values.
    * Example: `reset-config`

## CLI Configuration Examples

1.  **Configure an ESP as an AP that generates traffic to connecting stations and start collecting for 5 minutes:**
    ```
    set-wifi --mode ap
    set-wifi --ap-ssid=ESP_CSI_AP 
    set-wifi --ap-password=testing123
    set-traffic --frequency-hz=20
    show-config
    start --duration=300
    ```

2.  **Configure an ESP as a Station Monitor connected to an existing network, disable some CSI features, and start collecting indefinitely:**
    ```
    set-wifi --mode station 
    set-wifi --sta-ssid=My_Router
    set-wifi --sta-password=router_password
    set-csi --disable-htltf --disable-stbc-htltf
    show-config
    start
    ```

## Important Notes

> 🛑 SSIDs and passwords containing spaces must have the spaces replaced with underscores (`_`) when using the `set-wifi` command. The application will convert them back internally.

> 🛑 Ensure the target AP is running before starting collection in Station mode. Otherwise the application will `panic` as the station wont habe an AP to connect to.

## Building From Source (Optional)
Rather than downloading pre-built binaries, another approach is to clone the repository and build the source. This would require some additional dependencies and modifications depending on the device you are using.

### 📦 Dependencies
At a minimum, you would need the following:
* Rust toolchain with ESP target support installed. Full instructions for setting up a development environment are available [here](https://docs.esp-rs.org/book/installation/index.html). 
* Tool for flashing the firmware. It is recommended to use `esp-flash`. Installation instructions are available [here](https://docs.esp-rs.org/book/tooling/espflash.html).
* A terminal program to view the output. It is also recommended to use  `esp-flash` which was installed in the previous step.

### 📋 Procedure
1. ***Setup Project***: Clone this repository. The project repository code configuration defaults to the ESP32-C3 device. If you are using a ESP32-C3 you can skip to step 3, otherwise you need to modify the `config.toml` for your desired device. Also if you wish to enable `defmt` logging, make sure to read the following section.
2. ***Modify*** **`.cargo/config.toml`**: Head to the `config.toml` file and uncommment the runner and build target that aligns with the device you are using.
3. ***Build***: execute the following command in the terminal to build the project:
```bash
cargo build --features "[device name] [logging framework]" --release

# Example for the ESP32-C6 with println
cargo build --features "esp32c6 println" --release

# Example for the ESP32 with defmt
cargo build --features "esp32s3 defmt" --release
```
4. ***Monitor***: execute the following command in the terminal to run the project:
```bash
cargo run --features "[device name] [logging framework]" --release
```
## Enabling Logging w/ `defmt`
This application can use either the standard `println!` macros or the `defmt` framework for logging.

If you wan to enable `defmt` you need to make sure of the following in the `.cargo/config.toml`:
1.  **Runner Parameters are Included:** In `.cargo/config.toml` make sure `--log-format defmt` is added to the `runner` arguments as follows:
```
runner = "espflash flash --monitor --log-format defmt"
```
2.  **Linker Flags are Included:** In `.cargo/config.toml` make sure that `"-C link-arg=-Tdefmt.x"` is added to `rustflags` as follows:
```
rustflags = [
  "-C",
  "link-arg=-Tlinkall.x",
  "-C", 
  "link-arg=-Tdefmt.x",
]
```

## Documentation

This CLI is built around the esp-csi-rs crate. You can find full documentation for esp-csi-rs on [docs.rs](https://docs.rs/esp_csi_rs).

## Development

This crate is still in early development and currently supports `no-std` only. Contributions and suggestions are welcome!

## License
Copyright 2025 The Embedded Rustacean

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at
http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

---

Made with 🦀 for ESP chips
