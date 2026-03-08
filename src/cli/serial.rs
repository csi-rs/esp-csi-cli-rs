use esp_hal::uart::Uart;
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::Async;

#[cfg(feature = "esp32")]
pub type SerialInterface<'d> = Uart<'d, Async>;

// Not ESP32, no auto, and jtag-serial is explicitly requested
#[cfg(all(not(feature = "esp32"), not(feature = "auto"), feature = "jtag-serial"))]
pub type SerialInterface<'d> = UsbSerialJtag<'d, Async>;

// Not ESP32, no auto, and jtag-serial is NOT requested (fallback to UART)
#[cfg(all(not(feature = "esp32"), not(feature = "auto"), feature = "uart"))]
pub type SerialInterface<'d> = Uart<'d, Async>;

#[cfg(all(not(feature = "esp32"), feature = "auto"))]
pub enum SerialInterface<'d> {
    UsbJtag(UsbSerialJtag<'d, Async>),
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