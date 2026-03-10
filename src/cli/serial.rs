use esp_hal::uart::Uart;
#[cfg(not(feature = "esp32"))]
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::Async;

/// Serial interface type used by the CLI runner on ESP32 targets.
///
/// On ESP32, the USB-serial-JTAG peripheral is not available, so UART0 is always used.
#[cfg(feature = "esp32")]
pub type SerialInterface<'d> = Uart<'d, Async>;

/// Serial interface type used when `jtag-serial` is explicitly requested (non-ESP32).
#[cfg(all(not(feature = "esp32"), not(feature = "auto"), feature = "jtag-serial"))]
pub type SerialInterface<'d> = UsbSerialJtag<'d, Async>;

/// Serial interface type used when UART is explicitly requested (non-ESP32, no auto-detect).
#[cfg(all(not(feature = "esp32"), not(feature = "auto"), feature = "uart"))]
pub type SerialInterface<'d> = Uart<'d, Async>;

/// Runtime-selectable serial interface for targets that support both UART and USB-JTAG.
///
/// When the `auto` feature is enabled the correct backend is chosen at runtime by
/// [`is_jtag`]: if a USB host is detected the JTAG peripheral is used, otherwise
/// UART0 is used as a fallback.
#[cfg(all(not(feature = "esp32"), feature = "auto"))]
pub enum SerialInterface<'d> {
    /// USB Serial JTAG backend (faster, preferred when a USB host is present).
    UsbJtag(UsbSerialJtag<'d, Async>),
    /// UART0 backend (fallback when no USB host is detected).
    Uart(Uart<'d, Async>),
}

#[cfg(all(not(feature = "esp32"), feature = "auto"))]
impl<'d> core::fmt::Write for SerialInterface<'d> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        match self {
            Self::UsbJtag(j) => j.write_str(s),
            Self::Uart(u) => u.write_str(s),
        }
    }
}

#[cfg(all(not(feature = "esp32"), feature = "auto"))]
impl<'d> embedded_io::ErrorType for SerialInterface<'d> {
    type Error = embedded_io::ErrorKind;
}

#[cfg(all(not(feature = "esp32"), feature = "auto"))]
impl<'d> embedded_io_async::Read for SerialInterface<'d> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        use embedded_io::Error;

        match self {
            Self::UsbJtag(j) => embedded_io_async::Read::read(j, buf).await.map_err(|e| e.kind()),
            Self::Uart(u) => embedded_io_async::Read::read(u, buf).await.map_err(|e| e.kind()),
        }
    }
}

#[cfg(all(not(feature = "esp32"), feature = "auto"))]
impl<'d> embedded_io::Write for SerialInterface<'d> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        use embedded_io::Error;

        match self {
            Self::UsbJtag(j) => embedded_io::Write::write(j, buf).map_err(|e| e.kind()),
            Self::Uart(u) => embedded_io::Write::write(u, buf).map_err(|e| e.kind()),
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        use embedded_io::Error;

        match self {
            Self::UsbJtag(j) => embedded_io::Write::flush(j).map_err(|e| e.kind()),
            Self::Uart(u) => embedded_io::Write::flush(u).map_err(|e| e.kind()),
        }
    }
}

/// Detects at runtime whether a USB host is connected to the USB-Serial-JTAG peripheral.
///
/// Reads the `SOF_INT` bit from the device's `USB_DEVICE_INT_RAW` register.
/// A set bit means the USB host has sent a Start-Of-Frame packet, indicating
/// an active connection. This is used by the `auto` feature to choose between
/// the JTAG and UART backends without requiring a compile-time decision.
///
/// # Safety
/// Performs a raw memory-mapped register read. The address constants are
/// chip-specific and are selected via Cargo features at compile time.
///
/// Not available on ESP32 which has no USB-Serial-JTAG peripheral.
#[cfg(all(not(feature = "esp32"), feature = "auto"))]
pub fn is_jtag() -> bool {
    #[cfg(feature = "esp32c3")]
    const USB_DEVICE_INT_RAW: *const u32 = 0x60043008 as *const u32;
    #[cfg(feature = "esp32c6")]
    const USB_DEVICE_INT_RAW: *const u32 = 0x6000f008 as *const u32;
    #[cfg(feature = "esp32s3")]
    const USB_DEVICE_INT_RAW: *const u32 = 0x60038000 as *const u32;

    const SOF_INT_MASK: u32 = 0b10;
    unsafe { (USB_DEVICE_INT_RAW.read_volatile() & SOF_INT_MASK) != 0 }
}