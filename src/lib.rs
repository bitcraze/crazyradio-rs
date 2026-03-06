#![cfg_attr(docsrs, feature(doc_cfg))]

//! # Crazyradio driver for Rust
//!
//! This crate provides a **radio hardware abstraction** for the
//! [Crazyradio](https://www.bitcraze.io/products/crazyradio-pa/) USB dongle.
//!
//! Methods map to hardware operations (channels, datarates, TX power, addresses,
//! pipes) while USB protocol details — such as inline-mode bulk headers and
//! settings caching — are handled transparently.  Values use conventional
//! hardware-domain units (e.g. RSSI in negative dBm).  Higher-level concerns
//! like connection management, retry policies, and device discovery belong in
//! downstream crates such as `crazyflie-link`.
//!
//! # Cargo features
//!  - **shared_radio** enables [SharedCrazyradio] object that allows to share a radio between threads
//!  - **async** enables async versions of open/serial functions, the [SharedCrazyradio] async API, and async sniffer mode via [`Crazyradio::enter_sniffer_mode_async`]
//!  - **serde** emables [serde](https://crates.io/crates/serde) serialization/deserialization of the [Channel] struct
//!  - **packet_capture** enables functionality to capture packets by registering a callback which is called for each in/out packet

#![deny(missing_docs)]

#[cfg(feature = "shared_radio")]
mod shared_radio;
#[cfg(feature = "shared_radio")]
pub use crate::shared_radio::{SharedCrazyradio, WeakSharedCrazyradio};

#[cfg(feature = "packet_capture")]
pub mod capture;

#[cfg(feature = "async")]
mod async_sniffer;
#[cfg(feature = "async")]
pub use crate::async_sniffer::{ReceivedSnifferPacket, SnifferReceiver, SnifferSender};

use core::time::Duration;
use std::sync::Arc;
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
    SetRadioMode = 0x24,
    SetSnifferAddress = 0x25,
    GetSnifferDropCount = 0x26,
    SetPacketLossSimulation = 0x30,
    LaunchBootloader = 0xff,
}

/// Inline mode setting for USB protocol
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InlineMode {
    /// Inline mode disabled, all settings are set by control USB messages
    Off = 0,
    /// Inline mode enabled, channel, datarate, address and ack_enable are sent as header to the data in USB bulk messages
    On = 1,
    /// Inline mode enabled with RSSI, same as On but the returned ack header also contains the RSSI value of the received packet (if any)
    OnWithRssi = 2,
}

impl InlineMode {
    /// Returns `true` if the inline mode is on.
    pub fn is_on(&self) -> bool {
        matches!(self, InlineMode::On | InlineMode::OnWithRssi)
    }

    /// Returns `true` if the inline mode is off.
    pub fn is_off(&self) -> bool {
        matches!(self, InlineMode::Off)
    }
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
    device_handle: Arc<rusb::DeviceHandle<rusb::GlobalContext>>,

    cache_settings: bool,
    inline_mode: InlineMode,
    sniffer_mode: bool,

    // Settings cache
    channel: Channel,
    address: [u8; 5],
    datarate: Datarate,
    ack_enable: bool,

    /// Radio serial number (for capture identification)
    #[cfg(feature = "packet_capture")]
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
        let device_handle = Arc::new(device.open()?);

        device_handle.claim_interface(0)?;

        // Make sure the dongle version is >= 0.5
        let version = device_desciptor.device_version();
        let version = version.major() as f64
            + (version.minor() as f64 / 10.0)
            + (version.sub_minor() as f64 / 100.0);
        if version < 0.5 {
            return Err(Error::DongleVersionNotSupported);
        }

        #[cfg(feature = "packet_capture")]
        let serial = get_serial(&device_desciptor, &device_handle).unwrap_or_default();

        let mut cr = Crazyradio {
            device_desciptor,
            device_handle,

            cache_settings: true,
            inline_mode: InlineMode::Off,
            sniffer_mode: false,

            channel: Channel::from_number(2).unwrap(),
            address: [0xe7; 5],
            datarate: Datarate::Dr2M,

            ack_enable: true,

            #[cfg(feature = "packet_capture")]
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

        // Exit sniffer mode if active
        if self.sniffer_mode {
            let result = self.exit_sniffer_mode();
            // Always clear the flag regardless of USB outcome, so subsequent
            // calls are not permanently blocked on a failed reset.
            self.sniffer_mode = false;
            result?;
        }

        // Try to set inline mode, ignore failure as this is not fatal (old radio FW do not implement it and will just be slower)
        // We set it on first and then with rssi, this way the dongle is set to the maximum inline mode supported
        _ = self.set_inline_mode(InlineMode::On);
        _ = self.set_inline_mode(InlineMode::OnWithRssi);

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
        if self.inline_mode.is_off() && (!self.cache_settings || self.channel != channel) {
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
        if self.inline_mode.is_off() && (!self.cache_settings || self.datarate != datarate) {
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
        if self.inline_mode.is_off() && (!self.cache_settings || self.address != *address) {
            self.device_handle.write_control(
                0x40,
                UsbCommand::SetRadioAddress as u8,
                0,
                0,
                address,
                Duration::from_secs(1),
            )?;
        }

        if self.cache_settings || self.inline_mode.is_on() {
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
        if self.inline_mode.is_off() && ack_enable != self.ack_enable {
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
    pub fn set_inline_mode(&mut self, mode: InlineMode) -> Result<()> {
        let setting = mode as u16;

        self.device_handle.write_control(
            0x40,
            UsbCommand::SetInlineMode as u8,
            setting,
            0,
            &[],
            Duration::from_secs(1),
        )?;
        self.inline_mode = mode;

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

    /// Enter sniffer mode (continuous RX).
    ///
    /// The radio will passively listen for ESB packets on the configured
    /// channel, datarate, and address(es). Configure these using the standard
    /// `set_channel`, `set_datarate`, and `set_address` methods before calling
    /// this.
    ///
    /// Optionally set a second listening address on pipe 1 using
    /// `set_sniffer_address` before entering sniffer mode.
    ///
    /// While in sniffer mode, use `receive_sniffer_packet` to read packets.
    /// `send_packet` and `send_packet_no_ack` will return an error.
    pub fn enter_sniffer_mode(&mut self) -> Result<()> {
        // Disable inline mode so that cached settings are flushed to the radio
        if self.inline_mode.is_on() {
            let saved_inline_mode = self.inline_mode;
            self.set_inline_mode(InlineMode::Off)?;
            // Flush cached settings that were previously only sent inline
            let saved_cache_settings = self.cache_settings;
            self.cache_settings = false;
            self.set_channel(self.channel)?;
            self.set_datarate(self.datarate)?;
            self.set_address(&self.address.clone())?;
            self.set_ack_enable(self.ack_enable)?;
            self.cache_settings = saved_cache_settings;
            self.inline_mode = saved_inline_mode;
        }

        self.device_handle.write_control(
            0x40,
            UsbCommand::SetRadioMode as u8,
            1,
            0,
            &[],
            Duration::from_secs(1),
        )?;
        self.sniffer_mode = true;
        Ok(())
    }

    /// Exit sniffer mode and return to normal TX/ACK operation.
    ///
    /// Re-enables inline mode if it was active before entering sniffer mode.
    pub fn exit_sniffer_mode(&mut self) -> Result<()> {
        self.device_handle.write_control(
            0x40,
            UsbCommand::SetRadioMode as u8,
            0,
            0,
            &[],
            Duration::from_secs(1),
        )?;
        self.sniffer_mode = false;

        // Re-enable inline mode if it was previously active
        if self.inline_mode.is_on() {
            let mode = self.inline_mode;
            self.inline_mode = InlineMode::Off;
            self.set_inline_mode(mode)?;
        }

        Ok(())
    }

    /// Set the radio address for a sniffer pipe.
    ///
    /// Pipe 0 can also be set using the standard `set_address` method before
    /// entering sniffer mode. Pipe 1 allows listening on a second address
    /// simultaneously.
    pub fn set_sniffer_address(&mut self, pipe: u8, address: &[u8; 5]) -> Result<()> {
        if pipe > 1 {
            return Err(Error::InvalidArgument);
        }
        self.device_handle.write_control(
            0x40,
            UsbCommand::SetSnifferAddress as u8,
            pipe as u16,
            0,
            address,
            Duration::from_secs(1),
        )?;
        Ok(())
    }

    /// Get the number of packets dropped due to queue overflow since sniffer
    /// mode was last entered.
    pub fn get_sniffer_drop_count(&mut self) -> Result<u32> {
        let mut buf = [0u8; 4];
        self.device_handle.read_control(
            0xC0,
            UsbCommand::GetSnifferDropCount as u8,
            0,
            0,
            &mut buf,
            Duration::from_secs(1),
        )?;
        Ok(u32::from_le_bytes(buf))
    }

    /// Receive a single sniffed packet.
    ///
    /// Blocks until a packet is received or the timeout expires.
    /// Returns `Ok(None)` on timeout, `Ok(Some(packet))` on success.
    /// The payload is written into `payload_data` and its length is in
    /// `SnifferPacket::length`.
    ///
    /// Must be called while in sniffer mode (after `enter_sniffer_mode`).
    ///
    /// Important Note: If the length of the payload buffer passed to this
    /// function is smaller than the actual payload of the sniffed packet, the
    /// payload will be truncated to fit the buffer and no error will be returned.
    /// The length of the actual payload will still be correctly returned in
    /// `SnifferPacket::length`, so the caller can detect this case if needed.
    pub fn receive_sniffer_packet(
        &mut self,
        payload_data: &mut [u8],
        timeout: Duration,
    ) -> Result<Option<SnifferPacket>> {
        const SNIFFER_HEADER_LENGTH: usize = 7;

        if !self.sniffer_mode {
            return Err(Error::InvalidArgument);
        }

        let mut buf = [0u8; 39]; // 7 header + 32 max payload
        let received = match self.device_handle.read_bulk(0x81, &mut buf, timeout) {
            Ok(n) => n,
            Err(rusb::Error::Timeout) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        if received < SNIFFER_HEADER_LENGTH {
            return Err(Error::UsbProtocolError(
                "Sniffer packet too short".to_string(),
            ));
        }

        let total_length = buf[0] as usize;
        if total_length != received {
            return Err(Error::UsbProtocolError(
                "Sniffer packet length mismatch".to_string(),
            ));
        }

        let payload_length = total_length - SNIFFER_HEADER_LENGTH;
        let copy_len = payload_length.min(payload_data.len());
        payload_data[..copy_len]
            .copy_from_slice(&buf[SNIFFER_HEADER_LENGTH..SNIFFER_HEADER_LENGTH + copy_len]);

        let timestamp_us = u32::from_le_bytes([buf[3], buf[4], buf[5], buf[6]]);

        Ok(Some(SnifferPacket {
            rssi_dbm: -(buf[1] as i16),
            pipe: buf[2],
            timestamp_us,
            length: payload_length,
        }))
    }

    /// Send a broadcast (no-ack) packet while in sniffer mode.
    ///
    /// The packet is sent using the current channel, datarate, and pipe-0
    /// address. The radio briefly leaves RX mode during TX (~1ms), so
    /// incoming packets during this window will be missed.
    ///
    /// No response is sent on the IN endpoint.
    ///
    /// # Arguments
    ///
    ///  * `data`: 1 to 32 bytes of raw ESB payload to broadcast.
    pub fn send_sniffer_broadcast(&mut self, data: &[u8]) -> Result<()> {
        if !self.sniffer_mode {
            return Err(Error::InvalidArgument);
        }
        if data.is_empty() || data.len() > 32 {
            return Err(Error::InvalidArgument);
        }

        #[cfg(feature = "packet_capture")]
        capture::capture_packet(
            capture::DIRECTION_TX,
            self.channel.into(),
            &self.address,
            &self.serial,
            data,
        );

        self.device_handle
            .write_bulk(0x01, data, Duration::from_secs(1))?;

        Ok(())
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
        if self.sniffer_mode {
            return Err(Error::InvalidArgument);
        }

        // Capture TX packet
        #[cfg(feature = "packet_capture")]
        capture::capture_packet(
            capture::DIRECTION_TX,
            self.channel.into(),
            &self.address,
            &self.serial,
            data,
        );

        let ack = if self.inline_mode.is_on() {
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
                rssi_dbm: None,
            }
        };

        // Capture RX packet (ACK payload)
        #[cfg(feature = "packet_capture")]
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
        if self.sniffer_mode {
            return Err(Error::InvalidArgument);
        }

        // Capture TX packet
        #[cfg(feature = "packet_capture")]
        capture::capture_packet(
            capture::DIRECTION_TX,
            self.channel.into(),
            &self.address,
            &self.serial,
            data,
        );

        if self.inline_mode.is_on() {
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
        const IN_HEADER_RSSI_LENGTH: usize = 3;

        const OUT_FIELD2_ACK_ENABLE: u8 = 0x10;

        const IN_HEADER_ACK_RECEIVED: u8 = 0x01;
        const IN_HEADER_POWER_DETECTOR: u8 = 0x02;
        const _IN_HEADER_INVALID_SETTING: u8 = 0x04;
        const IN_HEADER_RETRY_MASK: u8 = 0xf0;
        const IN_HEADER_RETRY_SHIFT: u8 = 4;

        const IN_HEADER_RSSI: usize = 2;

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
        let answer_size =
            self.device_handle
                .read_bulk(0x81, &mut answer, Duration::from_secs(1))?;

        let header_length = match self.inline_mode {
            InlineMode::On => IN_HEADER_LENGTH,
            InlineMode::OnWithRssi => IN_HEADER_RSSI_LENGTH,
            InlineMode::Off => unreachable!(),
        };
        // The first byte of the answer is the size of the answer
        // The minimum possible answer is 2 bytes [size, header]
        if (answer_size < header_length) || ((answer[0] as usize) != answer_size) {
            return Err(Error::UsbProtocolError(
                "Inline header from radio malformed, try to update your radio".to_string(),
            ));
        }

        let ack_received = answer[1] & IN_HEADER_ACK_RECEIVED != 0;

        // Decode RSSI value if available
        let rssi_dbm = if self.inline_mode == InlineMode::OnWithRssi && ack_received {
            Some(-(answer[IN_HEADER_RSSI] as i16))
        } else {
            None
        };

        // Decode answer, at this point we are sure that answer[0] is >= 2
        let payload_length = (answer[0] as usize) - header_length;
        if let Some(ack_data) = ack_data {
            ack_data[0..payload_length]
                .copy_from_slice(&answer[header_length..(header_length + payload_length)]);
        }

        Ok(Ack {
            received: ack_received,
            power_detector: answer[1] & IN_HEADER_POWER_DETECTOR != 0,
            retry: ((answer[1] & IN_HEADER_RETRY_MASK) >> IN_HEADER_RETRY_SHIFT) as usize,
            length: payload_length,
            rssi_dbm,
        })
    }
}

/// # Async implementations
///
/// Async wrappers for blocking operations (open, serial listing) and async
/// sniffer mode entry.
///
/// The open/serial functions are implemented by spawning a thread and passing
/// the result back through a channel. This keeps the library
/// executor-independent.
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

    /// Enter sniffer mode and return async receiver/sender handles.
    ///
    /// Consumes the `Crazyradio` and returns a `(SnifferReceiver, SnifferSender)` pair.
    /// The receiver yields sniffed packets and is not `Clone` (single owner).
    /// The sender can be cloned and used to send broadcast packets concurrently.
    ///
    /// Use [`SnifferReceiver::close`] to exit sniffer mode and recover the `Crazyradio`.
    pub async fn enter_sniffer_mode_async(
        self,
    ) -> Result<(SnifferReceiver, SnifferSender)> {
        async_sniffer::enter_sniffer_mode_async(self).await
    }
}

/// Errors returned by Crazyradio functions
#[derive(thiserror::Error, Debug, Clone)]
#[non_exhaustive]
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
    /// USB protocol error, for example when receiving an answer of unexpected length
    #[error("USB protocol error ({0})")]
    UsbProtocolError(String),
    /// Sniffer session has been closed
    #[error("Sniffer session closed")]
    SnifferSessionClosed,
}

impl From<rusb::Error> for Error {
    fn from(usb_error: rusb::Error) -> Self {
        Error::UsbError(usb_error)
    }
}

/// Ack status of a sent packet
///
/// This struct contains information gathered by the radio about the transaction and the received ack packet (if any).
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
    /// RSSI in dBm (negative, e.g. -60 means -60 dBm).
    /// This is a measurement of the radio dongle of how strong the ack packet was received.
    /// This field is only available if the radio is set in InlineMode::OnWithRssi (default at value) and the radio firmware supports it (Crazyradio 2.0 with Fw >= 5.3).
    pub rssi_dbm: Option<i16>,
}

/// A packet received in sniffer mode
#[derive(Debug, Clone)]
pub struct SnifferPacket {
    /// RSSI in dBm (negative, e.g. -60 means -60 dBm)
    pub rssi_dbm: i16,
    /// Pipe index the packet was received on (0 or 1)
    pub pipe: u8,
    /// Timestamp in microseconds (wraps every ~71 minutes)
    pub timestamp_us: u32,
    /// Length of the payload
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
