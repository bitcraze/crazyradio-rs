#![cfg_attr(docsrs, feature(doc_cfg))]

//! # Crazyradio driver for Rust
//!
//! This crate aims at providing a Rust API for the [Crazyradio](https://www.bitcraze.io/products/crazyradio-pa/)
//! USB Dongle.
//!
//! Available Cargo features:
//!  - **shared_radio** enables [SharedCrazyradio] object that allows to share a radio between threads
//!  - **async** enables async function to create a [Crazyradio] object and use the [SharedCrazyradio]
//!  - **serde** emables [serde](https://crates.io/crates/serde) serialization/deserialization of the [Channel] struct
//!  - **wireshark** enables packet capture to Wireshark

#![deny(missing_docs)]

#[cfg(feature = "shared_radio")]
mod shared_radio;
#[cfg(feature = "shared_radio")]
pub use crate::shared_radio::{SharedCrazyradio, WeakSharedCrazyradio};

#[cfg(feature = "wireshark")]
pub mod capture;

use core::time::Duration;
#[cfg(feature = "serde_support")]
use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, Error>;

fn find_crazyradio(
    nth: Option<usize>,
    serial: Option<&str>,
) -> Result<rusb::Device<rusb::GlobalContext>> {
    let mut n = 0;

    for device in rusb::devices()?.iter() {
        let device_desc = device.device_descriptor()?;

        if device_desc.vendor_id() == 0x1915 && device_desc.product_id() == 0x7777 {
            let handle = device.open()?;

            if (nth == None || nth == Some(n))
                && (serial == None || serial == Some(&get_serial(&device_desc, &handle)?))
            {
                return Ok(device);
            }
            n += 1;
        }
    }
    Err(Error::NotFound)
}

fn get_serial<T: rusb::UsbContext>(
    device_desc: &rusb::DeviceDescriptor,
    handle: &rusb::DeviceHandle<T>,
) -> Result<String> {
    let languages = handle.read_languages(Duration::from_secs(1))?;

    if !languages.is_empty() {
        let serial =
            handle.read_serial_number_string(languages[0], device_desc, Duration::from_secs(1))?;
        Ok(serial)
    } else {
        Err(Error::NotFound)
    }
}

fn list_crazyradio_serials() -> Result<Vec<String>> {
    let mut serials = vec![];

    for device in rusb::devices()?.iter() {
        let device_desc = device.device_descriptor()?;

        if device_desc.vendor_id() == 0x1915 && device_desc.product_id() == 0x7777 {
            let handle: rusb::DeviceHandle<rusb::GlobalContext> = device.open()?;

            let languages = handle.read_languages(Duration::from_secs(1))?;

            if !languages.is_empty() {
                let serial = handle.read_serial_number_string(
                    languages[0],
                    &device_desc,
                    Duration::from_secs(1),
                )?;
                serials.push(serial);
            }
        }
    }
    Ok(serials)
}

enum UsbCommand {
    SetRadioChannel = 0x01,
    SetRadioAddress = 0x02,
    SetDataRate = 0x03,
    SetRadioPower = 0x04,
    SetRadioArd = 0x05,
    SetRadioArc = 0x06,
    AckEnable = 0x10,
    SetContCarrier = 0x20,
    // ScanChannels = 0x21,
    SetInlineMode = 0x23,
    SetPacketLossSimulation = 0x30,
    LaunchBootloader = 0xff,
}

/// Represents a Crazyradio
///
/// Holds the USB connection to a Crazyradio dongle.
/// The connection is closed when this object goes out of scope.Crazyradio
///
/// Usage example:
/// ```no_run
/// use crazyradio::{Crazyradio, Error, Channel};
///
/// fn main() -> Result<(), Error> {
///     let mut cr = Crazyradio::open_first()?;   // Open the first detected dongle
///
///     // Set the radio channel
///     cr.set_channel(Channel::from_number(42).unwrap());
///
///     // Send a `null` packet
///     let mut ack_data = [0u8; 32];
///     let ack = cr.send_packet(&[0xff], &mut ack_data)?;
///
///     println!("Ack received: {}, length: {}, data: {:?}", ack.received,
///                                                          ack.length,
///                                                          &ack_data[..ack.length]);
///
///     Ok(())
/// }
/// ```
pub struct Crazyradio {
    device_desciptor: rusb::DeviceDescriptor,
    device_handle: rusb::DeviceHandle<rusb::GlobalContext>,

    cache_settings: bool,
    inline_mode: bool,

    // Settings cache
    channel: Channel,
    address: [u8; 5],
    datarate: Datarate,
    ack_enable: bool,

    /// Radio serial number (for capture identification)
    #[cfg(feature = "wireshark")]
    serial: String,
}

impl Crazyradio {
    /// Open the first Crazyradio detected and returns a Crazyradio object.
    ///
    /// The dongle is reset to boot values before being returned
    pub fn open_first() -> Result<Self> {
        Crazyradio::open_nth(0)
    }

    /// Open the nth Crazyradio detected and returns a Crazyradio object.
    ///
    /// Radios are ordered appearance in the USB device list. This order is
    /// platform-specific.
    ///
    /// The dongle is reset to boot values before being returned
    pub fn open_nth(nth: usize) -> Result<Self> {
        Self::open_generic(Some(nth), None)
    }

    /// Open a Crazyradio by specifying its serial number
    ///
    /// Example:
    /// ```no_run
    /// use crazyradio::Crazyradio;
    /// # fn main() -> Result<(), crazyradio::Error> {
    /// let mut cr = Crazyradio::open_by_serial("FD61E54B7A")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn open_by_serial(serial: &str) -> Result<Self> {
        Self::open_generic(None, Some(serial))
    }

    // Generic version of the open function, called by the other open_* functions
    fn open_generic(nth: Option<usize>, serial: Option<&str>) -> Result<Self> {
        let device = find_crazyradio(nth, serial)?;

        let device_desciptor = device.device_descriptor()?;
        let device_handle = device.open()?;

        device_handle.claim_interface(0)?;

        // Make sure the dongle version is >= 0.5
        let version = device_desciptor.device_version();
        let version = version.major() as f64
            + (version.minor() as f64 / 10.0)
            + (version.sub_minor() as f64 / 100.0);
        if version < 0.5 {
            return Err(Error::DongleVersionNotSupported);
        }

        #[cfg(feature = "wireshark")]
        let serial = get_serial(&device_desciptor, &device_handle).unwrap_or_default();

        let mut cr = Crazyradio {
            device_desciptor,
            device_handle,

            cache_settings: true,
            inline_mode: false,

            channel: Channel::from_number(2).unwrap(),
            address: [0xe7; 5],
            datarate: Datarate::Dr2M,

            ack_enable: true,

            #[cfg(feature = "wireshark")]
            serial,
        };

        cr.reset()?;

        Ok(cr)
    }

    /// Return an ordered list of serial numbers of connected Crazyradios
    ///
    /// The order of the list is the same as accepted by the open_nth() function.
    pub fn list_serials() -> Result<Vec<String>> {
        list_crazyradio_serials()
    }

    /// Return the serial number of this radio
    pub fn serial(&self) -> Result<String> {
        get_serial(&self.device_desciptor, &self.device_handle)
    }

    /// Reset dongle parameters to boot values.
    ///
    /// This function is called by Crazyradio::open_*.
    pub fn reset(&mut self) -> Result<()> {
        let prev_cache_settings = self.cache_settings;
        self.cache_settings = false;

        // Try to set inline mode, ignore failure as this is not fatal (old radio FW do not implement it and will just be slower)
        _ = self.set_inline_mode(true);

        self.set_datarate(Datarate::Dr2M)?;
        self.set_channel(Channel::from_number(2).unwrap())?;
        self.set_cont_carrier(false)?;
        self.set_address(&[0xe7, 0xe7, 0xe7, 0xe7, 0xe7])?;
        self.set_power(Power::P0dBm)?;
        self.set_arc(3)?;
        self.set_ard_bytes(32)?;
        self.set_ack_enable(true)?;

        self.cache_settings = prev_cache_settings;

        Ok(())
    }

    /// Enable or disable caching of settings
    ///
    /// If enabled, setting the radio channel, address or datarate will be
    /// ignored if the settings is the same as the one already set in the dongle
    ///
    /// This is enabled by default and is a useful functionality to efficiently
    /// implement communication to multiple device as changing these settings
    /// require USB communication and is quite slow.
    pub fn set_cache_settings(&mut self, cache_settings: bool) {
        self.cache_settings = cache_settings;
    }

    /// Set the radio channel.
    pub fn set_channel(&mut self, channel: Channel) -> Result<()> {
        if !self.inline_mode && (!self.cache_settings || self.channel != channel) {
            self.device_handle.write_control(
                0x40,
                UsbCommand::SetRadioChannel as u8,
                channel.0 as u16,
                0,
                &[],
                Duration::from_secs(1),
            )?;
        }

        self.channel = channel;

        Ok(())
    }

    /// Set the datarate.
    pub fn set_datarate(&mut self, datarate: Datarate) -> Result<()> {
        if !self.inline_mode && (!self.cache_settings || self.datarate != datarate) {
            self.device_handle.write_control(
                0x40,
                UsbCommand::SetDataRate as u8,
                datarate as u16,
                0,
                &[],
                Duration::from_secs(1),
            )?;
        }

        self.datarate = datarate;

        Ok(())
    }

    /// Set the radio address.
    pub fn set_address(&mut self, address: &[u8; 5]) -> Result<()> {
        if !self.inline_mode && (!self.cache_settings || self.address != *address) {
            self.device_handle.write_control(
                0x40,
                UsbCommand::SetRadioAddress as u8,
                0,
                0,
                address,
                Duration::from_secs(1),
            )?;
        }

        if self.cache_settings || self.inline_mode {
            self.address.copy_from_slice(address);
        }

        Ok(())
    }

    /// Set the transmit power.
    pub fn set_power(&mut self, power: Power) -> Result<()> {
        self.device_handle.write_control(
            0x40,
            UsbCommand::SetRadioPower as u8,
            power as u16,
            0,
            &[],
            Duration::from_secs(1),
        )?;
        Ok(())
    }

    /// Set time to wait for the ack packet.
    pub fn set_ard_time(&mut self, delay: Duration) -> Result<()> {
        if delay <= Duration::from_millis(4000) {
            // Set to step above or equal to `delay`
            let ard = (delay.as_millis() as u16 / 250) - 1;
            self.device_handle.write_control(
                0x40,
                UsbCommand::SetRadioArd as u8,
                ard,
                0,
                &[],
                Duration::from_secs(1),
            )?;
            Ok(())
        } else {
            Err(Error::InvalidArgument)
        }
    }

    /// Set time to wait for the ack packet by specifying the max byte-length of the ack payload.
    pub fn set_ard_bytes(&mut self, nbytes: u8) -> Result<()> {
        if nbytes <= 32 {
            self.device_handle.write_control(
                0x40,
                UsbCommand::SetRadioArd as u8,
                0x80 | nbytes as u16,
                0,
                &[],
                Duration::from_secs(1),
            )?;
            Ok(())
        } else {
            Err(Error::InvalidArgument)
        }
    }

    /// Set the number of time the radio will retry to send the packet if an ack packet is not received in time.
    pub fn set_arc(&mut self, arc: usize) -> Result<()> {
        if arc <= 15 {
            self.device_handle.write_control(
                0x40,
                UsbCommand::SetRadioArc as u8,
                arc as u16,
                0,
                &[],
                Duration::from_secs(1),
            )?;
            Ok(())
        } else {
            Err(Error::InvalidArgument)
        }
    }

    /// Set if the radio waits for an ack packet.
    ///
    /// Should be disabled when sending broadcast packets.
    pub fn set_ack_enable(&mut self, ack_enable: bool) -> Result<()> {
        if !self.inline_mode && ack_enable != self.ack_enable {
            self.device_handle.write_control(
                0x40,
                UsbCommand::AckEnable as u8,
                ack_enable as u16,
                0,
                &[],
                Duration::from_secs(1),
            )?;
        }

        self.ack_enable = ack_enable;

        Ok(())
    }

    /// Sends a packet to a range of channel and returns a list of channel that acked
    ///
    /// Used to activally scann for receives on channels. This function sends
    pub fn scan_channels(
        &mut self,
        start: Channel,
        stop: Channel,
        packet: &[u8],
    ) -> Result<Vec<Channel>> {
        let mut ack_data = [0u8; 32];
        let mut result: Vec<Channel> = vec![];
        for ch in start.0..stop.0 + 1 {
            let channel = Channel::from_number(ch).unwrap();
            self.set_channel(channel)?;
            let ack = self.send_packet(packet, &mut ack_data)?;
            if ack.received {
                result.push(channel);
            }
        }
        Ok(result)
    }

    /// Launch the bootloader.
    ///
    /// Consumes the Crazyradio since it is not usable after that (it is in bootlaoder mode ...).
    pub fn launch_bootloader(self) -> Result<()> {
        self.device_handle.write_control(
            0x40,
            UsbCommand::LaunchBootloader as u8,
            0,
            0,
            &[],
            Duration::from_secs(1),
        )?;
        Ok(())
    }

    /// Set the radio in continious carrier mode.
    ///
    /// In continious carrier mode, the radio will transmit a continious sine
    /// wave at the setup channel frequency using the setup transmit power.
    pub fn set_cont_carrier(&mut self, enable: bool) -> Result<()> {
        self.device_handle.write_control(
            0x40,
            UsbCommand::SetContCarrier as u8,
            enable as u16,
            0,
            &[],
            Duration::from_secs(1),
        )?;
        Ok(())
    }

    /// Set inline-settings USB protocol mode
    ///
    /// When this mode is enabled, setting channel, datarate, address and
    /// ack_enable will become cached operations, and these settings
    /// will be sent as header to the data over USB. This increases performance
    /// when communicating with more than one PRX.
    ///
    /// This mode, if available, is activated by default when creating the Crazyradio
    /// object.
    ///
    /// This mode is only available with Crazyradio 2.0+
    pub fn set_inline_mode(&mut self, inline_mode_enable: bool) -> Result<()> {
        let setting = inline_mode_enable.then_some(1).unwrap_or(0);

        self.device_handle.write_control(
            0x40,
            UsbCommand::SetInlineMode as u8,
            setting,
            0,
            &[],
            Duration::from_secs(1),
        )?;
        self.inline_mode = inline_mode_enable;

        Ok(())
    }

    /// Set packet loss simulation.
    ///
    pub fn set_packet_loss_simulation(
        &mut self,
        packet_loss_percent: u8,
        ack_loss_percent: u8,
    ) -> Result<()> {
        if self.device_desciptor.device_version() < rusb::Version::from_bcd(0x0500) {
            return Err(Error::DongleVersionNotSupported);
        }

        if packet_loss_percent <= 100 && ack_loss_percent <= 100 {
            let data = [packet_loss_percent, ack_loss_percent];
            self.device_handle.write_control(
                0x40,
                UsbCommand::SetPacketLossSimulation as u8,
                0,
                0,
                &data,
                Duration::from_secs(1),
            )?;
            Ok(())
        } else {
            Err(Error::InvalidArgument)
        }
    }

    /// Send a data packet and receive an ack packet.
    ///
    /// # Arguments
    ///
    ///  * `data`: Up to 32 bytes of data to be send.
    ///  * `ack_data`: Buffer to hold the data received from the ack packet
    ///                payload. The ack payload can be up to 32 bytes, if this
    ///                buffer length is lower than 32 bytes the ack data might
    ///                be truncated. The length of the ack payload is returned
    ///                in Ack::length.
    pub fn send_packet(&mut self, data: &[u8], ack_data: &mut [u8]) -> Result<Ack> {
        // Capture TX packet
        #[cfg(feature = "wireshark")]
        capture::capture_packet(
            capture::DIRECTION_TX,
            self.channel.into(),
            &self.address,
            &self.serial,
            data,
        );

        let ack = if self.inline_mode {
            self.send_inline(data, Some(ack_data))?
        } else {
            self.device_handle
                .write_bulk(0x01, data, Duration::from_secs(1))?;
            let mut received_data = [0u8; 33];
            let received =
                self.device_handle
                    .read_bulk(0x81, &mut received_data, Duration::from_secs(1))?;

            if ack_data.len() <= 32 {
                ack_data.copy_from_slice(&received_data[1..ack_data.len() + 1]);
            } else {
                ack_data
                    .split_at_mut(32)
                    .0
                    .copy_from_slice(&received_data[1..33]);
            }

            Ack {
                received: received_data[0] & 0x01 != 0,
                power_detector: received_data[0] & 0x02 != 0,
                retry: ((received_data[0] & 0xf0) >> 4) as usize,
                length: received - 1,
            }
        };

        // Capture RX packet (ACK payload)
        #[cfg(feature = "wireshark")]
        if ack.received && ack.length > 0 {
            capture::capture_packet(
                capture::DIRECTION_RX,
                self.channel.into(),
                &self.address,
                &self.serial,
                &ack_data[..ack.length.min(ack_data.len())],
            );
        }

        Ok(ack)
    }

    /// Send a data packet without caring for Ack (for broadcast communication).
    ///
    /// # Arguments
    ///
    ///  * `data`: Up to 32 bytes of data to be send.
    pub fn send_packet_no_ack(&mut self, data: &[u8]) -> Result<()> {
        // Capture TX packet
        #[cfg(feature = "wireshark")]
        capture::capture_packet(
            capture::DIRECTION_TX,
            self.channel.into(),
            &self.address,
            &self.serial,
            data,
        );

        if self.inline_mode {
            self.send_inline(data, None)?;
        } else {
            self.device_handle
                .write_bulk(0x01, data, Duration::from_secs(1))?;
        }

        Ok(())
    }

    fn send_inline(&mut self, data: &[u8], ack_data: Option<&mut [u8]>) -> Result<Ack> {
        const OUT_HEADER_LENGTH: usize = 8;
        const IN_HEADER_LENGTH: usize = 2;

        const OUT_FIELD2_ACK_ENABLE: u8 = 0x10;

        const IN_HEADER_ACK_RECEIVED: u8 = 0x01;
        const IN_HEADER_POWER_DETECTOR: u8 = 0x02;
        const _IN_HEADER_INVALID_SETTING: u8 = 0x04;
        const IN_HEADER_RETRY_MASK: u8 = 0xf0;
        const IN_HEADER_RETRY_SHIFT: u8 = 4;

        // Assemble out command
        let mut command = vec![];
        command.push((OUT_HEADER_LENGTH + data.len()) as u8);
        let mut field2 = self.datarate as u8;
        if self.ack_enable {
            field2 |= OUT_FIELD2_ACK_ENABLE;
        }
        command.push(field2);
        command.push(self.channel.into());
        command.extend_from_slice(&self.address);
        command.extend_from_slice(&data);

        let mut answer = [0u8; 64];
        self.device_handle
            .write_bulk(0x01, &command, Duration::from_secs(1))?;
        self.device_handle
            .read_bulk(0x81, &mut answer, Duration::from_secs(1))?;

        // Decode answer
        let payload_length = (answer[0] as usize) - 2;
        if let Some(ack_data) = ack_data {
            ack_data[0..payload_length]
                .copy_from_slice(&answer[IN_HEADER_LENGTH..(IN_HEADER_LENGTH + payload_length)]);
        }

        Ok(Ack {
            received: answer[1] & IN_HEADER_ACK_RECEIVED != 0,
            power_detector: answer[1] & IN_HEADER_POWER_DETECTOR != 0,
            retry: ((answer[1] & IN_HEADER_RETRY_MASK) >> IN_HEADER_RETRY_SHIFT) as usize,
            length: payload_length,
        })
    }
}

/// # Async implementations
///
/// Async version of open/getserial functions.
///
/// Implemented by launching a thread, calling the sync function and passing the
/// result back though a channel.
/// This is not the most efficient implementation but it keeps the lib executor-independent
/// and these functions are only one-time-call in most programs.
#[cfg(feature = "async")]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
impl Crazyradio {
    /// Async vesion of [Crazyradio::open_first()]
    pub async fn open_first_async() -> Result<Self> {
        let (tx, rx) = flume::bounded(0);

        std::thread::spawn(move || tx.send(Self::open_first()));

        rx.recv_async().await.unwrap()
    }

    /// Async vesion of [Crazyradio::open_nth()]
    pub async fn open_nth_async(nth: usize) -> Result<Self> {
        let (tx, rx) = flume::bounded(0);

        std::thread::spawn(move || tx.send(Self::open_nth(nth)));

        rx.recv_async().await.unwrap()
    }

    /// Async vesion of [Crazyradio::open_by_serial()]
    pub async fn open_by_serial_async(serial: &str) -> Result<Self> {
        let serial = serial.to_owned();

        let (tx, rx) = flume::bounded(0);

        std::thread::spawn(move || tx.send(Self::open_by_serial(&serial)));

        rx.recv_async().await.unwrap()
    }

    /// Async vesion of [Crazyradio::list_serials()]
    pub async fn list_serials_async() -> Result<Vec<String>> {
        let (tx, rx) = flume::bounded(0);

        std::thread::spawn(move || tx.send(Self::list_serials()));

        rx.recv_async().await.unwrap()
    }
}

/// Errors returned by Crazyradio functions
#[derive(thiserror::Error, Debug, Clone)]
pub enum Error {
    /// USB error returned by the underlying rusb library
    #[error("Usb Error: {0:?}")]
    UsbError(rusb::Error),
    /// Crazyradio not found
    #[error("Crazyradio not found")]
    NotFound,
    /// Invalid argument passed to function
    #[error("Invalid arguments")]
    InvalidArgument,
    /// Crazyradio version not supported
    #[error("Crazyradio version not supported")]
    DongleVersionNotSupported,
}

impl From<rusb::Error> for Error {
    fn from(usb_error: rusb::Error) -> Self {
        Error::UsbError(usb_error)
    }
}

/// Ack status of a sent packet
#[derive(Debug, Copy, Clone)]
pub struct Ack {
    /// At true if an ack packet has been received
    pub received: bool,
    /// Value of the nRF24 power detector when receiving the ack packet
    pub power_detector: bool,
    /// Number of time the packet was sent before an ack was received
    pub retry: usize,
    /// Length of the ack payload
    pub length: usize,
}

/// Radio channel
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde_support", derive(Serialize))]
pub struct Channel(u8);

#[cfg(feature = "serde_support")]
impl<'de> Deserialize<'de> for Channel {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Channel, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let ch_number: u8 = Deserialize::deserialize(deserializer)?;
        let channel = Channel::from_number(ch_number)
            .map_err(|e| serde::de::Error::custom(format!("{:?}", e)))?;
        Ok(channel)
    }
}

impl Channel {
    /// Create a Channel from its number (0-125)
    ///
    /// Returns an Error::InvalidArgument if the channel number is out of range
    pub fn from_number(channel: u8) -> Result<Self> {
        if channel < 126 {
            Ok(Channel(channel))
        } else {
            Err(Error::InvalidArgument)
        }
    }
}

impl From<Channel> for u8 {
    fn from(val: Channel) -> Self {
        val.0
    }
}

/// Radio datarate
#[derive(Copy, Clone, PartialEq)]
pub enum Datarate {
    /// 250 kbps
    Dr250K = 0,
    /// 1 Mbps
    Dr1M = 1,
    /// 2 Mbps
    Dr2M = 2,
}

/// Radio power
pub enum Power {
    /// -18 dBm
    Pm18dBm = 0,
    /// -12 dBm
    Pm12dBm = 1,
    /// -6 dBm
    Pm6dBm = 2,
    /// 0 dBm
    P0dBm = 3,
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "serde_support")]
    use serde_json;

    #[cfg(feature = "serde_support")]
    use super::Channel;

    #[test]
    #[cfg(feature = "serde_support")]
    fn test_that_deserializing_a_correct_channel_works() {
        let test_str = "42";

        let result: Result<Channel, serde_json::Error> = serde_json::from_str(test_str);

        assert!(matches!(result, Ok(Channel(42))));
    }

    #[test]
    #[cfg(feature = "serde_support")]
    fn test_that_deserializing_an_incorrect_channel_works() {
        let test_str = "126";

        let result: Result<Channel, serde_json::Error> = serde_json::from_str(test_str);

        assert!(matches!(result, Err(_)));
    }

    #[test]
    #[cfg(feature = "serde_support")]
    fn test_that_serialize_channel_works() {
        let test_channel = Channel(42);

        let result = serde_json::to_string(&test_channel);

        assert!(matches!(result, Ok(str) if str == "42"));
    }
}
