
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
    ScanChannels = 0x21,
    LaunchBootloader = 0xff,
}

pub struct Crazyradio {
    pub device: rusb::Device<rusb::GlobalContext>,
    pub device_handle: rusb::DeviceHandle<rusb::GlobalContext>,
}

impl Crazyradio {
    pub fn new() -> Result<Self, Error> {
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

    pub fn reset(&mut self) {
        todo!();
    }

    pub fn set_channel(&mut self, channel: Channel) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::SetRadioChannel as u8, channel.into(), 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

    pub fn set_datarate(&mut self, datarate: Datarate) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::SetDataRate as u8, datarate as u16, 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

    pub fn set_address(&mut self, address: &[u8; 5]) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::SetRadioAddress as u8, 0, 0, address, Duration::from_secs(1))?;
        Ok(())
    }

    pub fn set_power(&mut self, power: Power) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::SetRadioPower as u8, power as u16, 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

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

    pub fn set_ard_bytes(&mut self, nbytes: u8) -> Result<(), Error> {
        if nbytes <= 32 {
            self.device_handle.write_control(0x40, UsbCommand::SetRadioArd as u8, 0x80 | nbytes as u16, 0, &[], Duration::from_secs(1))?;
            Ok(())
        } else {
            Err(Error::InvalidArgument)
        }
    }

    pub fn set_arc(&mut self, arc: usize) -> Result<(), Error> {
        if arc <= 15 {
            self.device_handle.write_control(0x40, UsbCommand::SetRadioArc as u8, arc as u16, 0, &[], Duration::from_secs(1))?;
            Ok(())
        } else {
            Err(Error::InvalidArgument)
        }
    }

    pub fn set_ack_enable(&mut self, ack_enable: bool) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::AckEnable as u8, ack_enable as u16, 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

    pub fn scan_channels(&mut self, start: Channel, stop: Channel, packet: &[u8]) -> Result<Vec<Channel>, Error> {
        // Start the scann
        self.device_handle.write_control(0x40, UsbCommand::ScanChannels as u8, start.into(), stop.into(), packet, Duration::from_secs(1))?;
        // Get the result
        let mut raw_result = [0u8, 64];
        let raw_length = self.device_handle.read_control(0xC0, UsbCommand::ScanChannels as u8, 0, 0, &mut raw_result, Duration::from_secs(1))?;
        dbg!(raw_length);
        if raw_length > 63 {
            // On some host, an empty answer results in a 64 bytes packet
            // Filter all 64 bytes answers as empty
            Ok(vec![])
        } else {
            Ok(raw_result[..raw_length].into_iter().map(|c| Channel::new(*c).unwrap()).collect())
        }
    }

    // Launch bootloader consumes the radio since it is not usable after that (it is in bootlaoder mode ...)
    pub fn launch_bootloader(self) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::LaunchBootloader as u8, 0, 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

    pub fn set_cont_carrier(&mut self, enable: bool) -> Result<(), Error> {
        self.device_handle.write_control(0x40, UsbCommand::SetContCarrier as u8, enable as u16, 0, &[], Duration::from_secs(1))?;
        Ok(())
    }

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
    pub fn new(channel: u8) -> Option<Self> {
        if channel < 126 {
            Some(Channel(channel))
        } else {
            None
        }
    }
}

impl Into<u16> for Channel {
    fn into(self) -> u16 { self.0 as u16 }
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
