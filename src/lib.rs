
use rusb;
use core::time::Duration;

fn find_crazyradio() -> Option<rusb::Device<rusb::GlobalContext>> {
    for device in rusb::devices().unwrap().iter() {
        let device_desc = device.device_descriptor().unwrap();

        if device_desc.vendor_id() == 0x1915 && device_desc.product_id() == 0x7777 {
            return Some(device);
        }
    }
    return None;
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
///     let ack_length = cr.send_packet(&[0xff], &mut ack_data)?;
/// 
///     Ok(())
/// }
/// ```
pub struct Crazyradio {
    device: rusb::Device<rusb::GlobalContext>,
    device_handle: rusb::DeviceHandle<rusb::GlobalContext>,
}

impl Crazyradio {

    /// Open the first Crazyradio detected and returns a Crazyradio object.println!
    /// 
    /// The dongle is reset to boot values before being returned
    pub fn open_first() -> Result<Self, Error> {
        if let Some(device) = find_crazyradio() {
            let device_handle = device.open()?;

            Ok(Crazyradio {
                device,
                device_handle,
            })
        } else {
            Err(Error::NotFound)
        }
    }

    /// Reset dongle parameters to boot values.
    /// 
    /// This function is called by Crazyradio::new.
    pub fn reset(&mut self) {
        todo!();
    }

    /// Set the radio channel.
    pub fn set_channel(&mut self, channel: Channel) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::SetRadioChannel as u8, channel.0 as u16, 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

    /// Set the datarate.
    pub fn set_datarate(&mut self, datarate: Datarate) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::SetDataRate as u8, datarate as u16, 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

    /// Set the radio address.
    pub fn set_address(&mut self, address: &[u8; 5]) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::SetRadioAddress as u8, 0, 0, address, Duration::from_secs(1))?;
        Ok(())
    }

    /// Set the transmit power.
    pub fn set_power(&mut self, power: Power) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::SetRadioPower as u8, power as u16, 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

    /// Set time to wait for the ack packet.
    pub fn set_ard_time(&mut self, delay: Duration) -> Result<(), Error> {
        if delay <= Duration::from_millis(4000) {
            // Set to step above or equal to `delay`
            let ard = (delay.as_millis() as u16 /250) - 1;
            self.device_handle.write_control(0x40, UsbCommand::SetRadioArd as u8, ard, 0, &[], Duration::from_secs(1))?;
            Ok(())
        } else {
            Err(Error::InvalidArgument)
        }
    }

    /// Set the number of bytes to wait for when waiting for the ack packet.
    pub fn set_ard_bytes(&mut self, nbytes: u8) -> Result<(), Error> {
        if nbytes <= 32 {
            self.device_handle.write_control(0x40, UsbCommand::SetRadioArd as u8, 0x80 | nbytes as u16, 0, &[], Duration::from_secs(1))?;
            Ok(())
        } else {
            Err(Error::InvalidArgument)
        }
    }

    /// Set the number of time the radio will retry to send the packet if an ack packet is not received in time.
    pub fn set_arc(&mut self, arc: usize) -> Result<(), Error> {
        if arc <= 15 {
            self.device_handle.write_control(0x40, UsbCommand::SetRadioArc as u8, arc as u16, 0, &[], Duration::from_secs(1))?;
            Ok(())
        } else {
            Err(Error::InvalidArgument)
        }
    }

    /// Set if the radio waits for an ack packet.
    /// 
    /// Should be disabled when sending broadcast packets.
    pub fn set_ack_enable(&mut self, ack_enable: bool) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::AckEnable as u8, ack_enable as u16, 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

    /// Sends a packet to a range of channel and returns a list of channel that acked
    /// 
    /// Used to activally scann for receives on channels. This function sends
    pub fn scan_channels(&mut self, start: Channel, stop: Channel, packet: &[u8]) -> Result<Vec<Channel>, Error> {
        let mut ack_data = [0u8; 32];
        let mut result: Vec<Channel> = vec![];
        for ch in start.0..stop.0+1 {
            let channel = Channel::from_number(ch).unwrap();
            self.set_channel(channel)?;
            let n_received = self.send_packet(packet, &mut ack_data)?;
            if n_received > 0 {
                result.push(channel);
            }
        }
        Ok(result)
    }

    /// Launch the bootloader.
    /// 
    /// Consumes the Crazyradio since it is not usable after that (it is in bootlaoder mode ...).
    pub fn launch_bootloader(self) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::LaunchBootloader as u8, 0, 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

    /// Set the radio in continious carrier mode.
    /// 
    /// In continious carrier mode, the radio will transmit a continious sine
    /// wave at the setup channel frequency using the setup transmit power.
    pub fn set_cont_carrier(&mut self, enable: bool) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::SetContCarrier as u8, enable as u16, 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

    /// Send a data packet and receive an ack packet.
    pub fn send_packet(&mut self, data: &[u8], ack_data: &mut [u8; 32]) -> Result<usize, Error> {
        self.device_handle.write_bulk(0x01, data, Duration::from_secs(1))?;
        let mut received_data = [0u8; 33];
        let received = self.device_handle.read_bulk(0x81, &mut received_data, Duration::from_secs(1))?;

        ack_data.copy_from_slice(&received_data[1..]);

        Ok(received-1)
    }
}

#[derive(Debug, Clone)]
pub enum Error {
    UsbError(rusb::Error),
    NotFound,
    InvalidArgument,
}

impl From<rusb::Error> for Error {
    fn from(usb_error: rusb::Error) -> Self { Error::UsbError(usb_error) }
}

#[derive(Debug, Copy, Clone)]
pub struct Channel(u8);

impl Channel {
    pub fn from_number(channel: u8) -> Result<Self, Error> {
        if channel < 126 {
            Ok(Channel(channel))
        } else {
            Err(Error::InvalidArgument)
        }
    }
}

pub enum Datarate {
    Dr250K = 0,
    Dr1M = 1,
    Dr2M = 2,
}

pub enum Power {
    Pm18dBm = 0,
    Pm12dBm = 1,
    Pm6dBm = 2,
    P0dBm = 3,
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
