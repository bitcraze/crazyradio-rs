#![cfg(feature = "shared_radio")]
#![cfg_attr(docsrs, doc(cfg(feature = "shared_radio")))]

use crate::Result;
use crate::{Ack, Channel, Crazyradio};
use flume::{bounded, unbounded, Receiver, Sender, WeakSender};

/// Multi-user threaded Crazyradio
///
/// Runs the radio USB communication in a thread and
/// allows other threads or async tasks to send/receiver packets and scan.
///
/// When created, this object takes ownership of the radio.
/// To allow more user of the radio, simply clone the SharedCrazyradio
/// object. When the last SharedCrazyradio is dropped, the communication
/// thread is stopped and the radio object is dropped which
/// closes the USB connection.
///
/// Usage example:
/// ``` no_run
/// let radio = crazyradio::Crazyradio::open_first().unwrap();
/// let mut radio_thread = crazyradio::SharedCrazyradio::new(radio);
///
/// let mut radio_thread2 = radio_thread.clone();
///
/// std::thread::spawn(move || {
///     loop {
///         radio_thread2.send_packet(crazyradio::Channel::from_number(42).unwrap(), [0xe7;5], vec![0xff]);
///         std::thread::sleep(std::time::Duration::from_millis(333))
///     }
/// });
///
/// loop {
///     radio_thread.send_packet(crazyradio::Channel::from_number(42).unwrap(), [0xe7;5], vec![0xff]);
///     std::thread::sleep(std::time::Duration::from_millis(500))
/// }
///
pub struct SharedCrazyradio {
    radio_command: Sender<RadioCommand>,
    send_packet_res_send: Sender<Result<SendPacketResult>>,
    send_packet_res: Receiver<Result<SendPacketResult>>,
    send_packet_no_ack_res_send: Sender<Result<()>>,
    send_packet_no_ack_res: Receiver<Result<()>>,
    scan_res_send: Sender<Result<ScanResult>>,
    scan_res: Receiver<Result<ScanResult>>,
}

impl SharedCrazyradio {
    /// Create a shared crazyradio. The Shared Crazyradio takes ownership of the
    /// Crazyradio object to that it is not usable outside anymore.
    ///
    /// Will spawn a thread that service the radio requests. The radio can be
    /// shared by cloning the [SharedCrazyradio] object. When the last object
    /// is dropped, the thread will be closed and the Crazyradio is dropped as
    /// well closing the USB connection to it.
    pub fn new(radio: Crazyradio) -> Self {
        let (radio_command, radio_command_recv) = unbounded();

        std::thread::spawn(move || {
            radio_loop(radio, radio_command_recv);
        });

        let (send_packet_res_send, send_packet_res) = bounded(1);
        let (send_packet_no_ack_res_send, send_packet_no_ack_res) = bounded(1);
        let (scan_res_send, scan_res) = bounded(1);

        SharedCrazyradio {
            radio_command,
            send_packet_res_send,
            send_packet_res,
            send_packet_no_ack_res_send,
            send_packet_no_ack_res,
            scan_res_send,
            scan_res,
        }
    }

    /// Scan channels between start and stop for a specified address and payload.
    /// Internally it sets the address and calls [Crazyradio::scan_channels()].
    ///
    /// This function is atomic, this means that the radio will be taken for the
    /// whole duration of the scan. The intention is that scan are rare and done
    /// before any connection are active.
    pub fn scan(
        &self,
        start: Channel,
        stop: Channel,
        address: [u8; 5],
        payload: Vec<u8>,
    ) -> Result<Vec<Channel>> {
        self.radio_command
            .send(RadioCommand::Scan {
                client: self.scan_res_send.clone(),
                start,
                stop,
                address,
                payload,
            })
            .unwrap();

        let result = self.scan_res.recv().unwrap()?;

        Ok(result.found)
    }

    /// Send a packet to a `channel`, `address` containing `payload`.
    ///
    /// Returns an [Ack] struct containing information about the ack packet as
    /// well as the data content of the ack packet if an ack has been received.
    ///
    /// Can return any error the [Crazyradio::send_packet()] can return. This is
    /// mostly USB communication errors if the Crazyradio is disconnected.
    pub fn send_packet(
        &mut self,
        channel: Channel,
        address: [u8; 5],
        payload: Vec<u8>,
    ) -> Result<(Ack, Vec<u8>)> {
        self.radio_command
            .send(RadioCommand::SendPacket {
                client: self.send_packet_res_send.clone(),
                channel,
                address,
                payload,
            })
            .unwrap();

        let result = self.send_packet_res.recv().unwrap()?;

        Ok((
            Ack {
                received: result.acked,
                length: result.payload.len(),
                power_detector: result.power_detector,
                retry: result.retry,
                rssi_dbm: result.rssi_dbm,
            },
            result.payload,
        ))
    }

    /// Send a packet to a `channel`, `address` containing `payload` without caring about an Ack.
    ///
    /// Can return any error the [Crazyradio::send_packet_no_ack()] can return. This is
    /// mostly USB communication errors if the Crazyradio is disconnected.
    pub fn send_packet_no_ack(
        &mut self,
        channel: Channel,
        address: [u8; 5],
        payload: Vec<u8>,
    ) -> Result<()> {
        self.radio_command
            .send(RadioCommand::SendPacketNoAck {
                client: self.send_packet_no_ack_res_send.clone(),
                channel,
                address,
                payload,
            })
            .unwrap();
        Ok(())
    }

    /// Create a weak reference to this SharedCrazyradio.
    ///
    /// The weak reference can be upgraded to a SharedCrazyradio if the radio thread
    /// is still alive.
    ///
    /// The Radio thread is closed as soon as all SharedCrazyradio instances are dropped.
    pub fn downgrade(&self) -> WeakSharedCrazyradio {
        WeakSharedCrazyradio {
            radio_command: Some(self.radio_command.downgrade()),
        }
    }
}

#[cfg(feature = "async")]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
impl SharedCrazyradio {
    /// Async version of `scan()`
    pub async fn scan_async(
        &mut self,
        start: Channel,
        stop: Channel,
        address: [u8; 5],
        payload: Vec<u8>,
    ) -> Result<Vec<Channel>> {
        self.radio_command
            .send_async(RadioCommand::Scan {
                client: self.scan_res_send.clone(),
                start,
                stop,
                address,
                payload,
            })
            .await
            .unwrap();

        let result = self.scan_res.recv_async().await.unwrap()?;

        Ok(result.found)
    }

    /// Async version of `send_packet()`
    pub async fn send_packet_async(
        &mut self,
        channel: Channel,
        address: [u8; 5],
        payload: Vec<u8>,
    ) -> Result<(Ack, Vec<u8>)> {
        self.radio_command
            .send_async(RadioCommand::SendPacket {
                client: self.send_packet_res_send.clone(),
                channel,
                address,
                payload,
            })
            .await
            .unwrap();

        let result = self.send_packet_res.recv_async().await.unwrap()?;

        Ok((
            Ack {
                received: result.acked,
                length: result.payload.len(),
                power_detector: result.power_detector,
                retry: result.retry,
                rssi_dbm: result.rssi_dbm,
            },
            result.payload,
        ))
    }

    /// Async version of `send_packet_no_ack()`
    pub async fn send_packet_no_ack_async(
        &mut self,
        channel: Channel,
        address: [u8; 5],
        payload: Vec<u8>,
    ) -> Result<()> {
        self.radio_command
            .send_async(RadioCommand::SendPacketNoAck {
                client: self.send_packet_no_ack_res_send.clone(),
                channel,
                address,
                payload,
            })
            .await
            .unwrap();

        self.send_packet_no_ack_res.recv_async().await.unwrap()?;

        Ok(())
    }
}

impl Clone for SharedCrazyradio {
    fn clone(&self) -> Self {
        // Create new pair of return channels
        let (send_packet_res_send, send_packet_res) = bounded(1);
        let (send_packet_no_ack_res_send, send_packet_no_ack_res) = bounded(1);
        let (scan_res_send, scan_res) = bounded(1);

        // The command channel is cloned
        let radio_command = self.radio_command.clone();

        SharedCrazyradio {
            radio_command,
            send_packet_res_send,
            send_packet_res,
            send_packet_no_ack_res_send,
            send_packet_no_ack_res,
            scan_res_send,
            scan_res,
        }
    }
}

/// A weak reference to a [SharedCrazyradio]
///
/// Can be upgraded to a [SharedCrazyradio] if the radio thread is still alive.
///
/// This is useful to make sure the radio usb device is closed as soon as all
/// `SharedCrazyradio` instances are dropped.
pub struct WeakSharedCrazyradio {
    radio_command: Option<WeakSender<RadioCommand>>,
}

impl Default for WeakSharedCrazyradio {
    fn default() -> Self {
        WeakSharedCrazyradio {
            radio_command: None,
        }
    }
}

impl WeakSharedCrazyradio {
    /// Create a `SharedCrazyradio` from a weak reference.
    ///
    /// Returns `None` if the radio thread has been closed. Otherwise returns
    /// a new `SharedCrazyradio` instance that can be used to use the radio.
    pub fn upgrade(&self) -> Option<SharedCrazyradio> {
        let radio_command = self.radio_command.as_ref()?.upgrade()?;

        // Create new pair of return channels
        let (send_packet_res_send, send_packet_res) = bounded(1);
        let (send_packet_no_ack_res_send, send_packet_no_ack_res) = bounded(1);
        let (scan_res_send, scan_res) = bounded(1);

        Some(SharedCrazyradio {
            radio_command,
            send_packet_res_send,
            send_packet_res,
            send_packet_no_ack_res_send,
            send_packet_no_ack_res,
            scan_res_send,
            scan_res,
        })
    }
}

enum RadioCommand {
    SendPacket {
        client: Sender<Result<SendPacketResult>>,
        channel: Channel,
        address: [u8; 5],
        payload: Vec<u8>,
    },
    SendPacketNoAck {
        client: Sender<Result<()>>,
        channel: Channel,
        address: [u8; 5],
        payload: Vec<u8>,
    },
    Scan {
        client: Sender<Result<ScanResult>>,
        start: Channel,
        stop: Channel,
        address: [u8; 5],
        payload: Vec<u8>,
    },
}

struct SendPacketResult {
    acked: bool,
    payload: Vec<u8>,
    retry: usize,
    power_detector: bool,
    rssi_dbm: Option<u8>,
}
struct ScanResult {
    found: Vec<Channel>,
}

fn scan(
    crazyradio: &mut Crazyradio,
    start: Channel,
    stop: Channel,
    address: [u8; 5],
    payload: Vec<u8>,
) -> Result<ScanResult> {
    crazyradio.set_address(&address)?;
    let found = crazyradio.scan_channels(start, stop, &payload)?;

    Ok(ScanResult { found })
}

fn send_packet(
    crazyradio: &mut Crazyradio,
    channel: Channel,
    address: [u8; 5],
    payload: Vec<u8>,
) -> Result<SendPacketResult> {
    let mut ack_data = Vec::new();
    ack_data.resize(32, 0);
    crazyradio.set_channel(channel)?;
    crazyradio.set_address(&address)?;
    crazyradio.set_ack_enable(true)?;

    let ack = crazyradio.send_packet(&payload, &mut ack_data)?;
    ack_data.resize(ack.length, 0);

    Ok(SendPacketResult {
        acked: ack.received,
        payload: ack_data,
        retry: ack.retry,
        power_detector: ack.power_detector,
        rssi_dbm: ack.rssi_dbm,
    })
}

fn send_packet_no_ack(
    crazyradio: &mut Crazyradio,
    channel: Channel,
    address: [u8; 5],
    payload: Vec<u8>,
) -> Result<()> {
    crazyradio.set_channel(channel)?;
    crazyradio.set_address(&address)?;
    crazyradio.set_ack_enable(false)?;

    crazyradio.send_packet_no_ack(&payload)
}

fn radio_loop(crazyradio: Crazyradio, radio_cmd: Receiver<RadioCommand>) {
    let mut crazyradio = crazyradio;
    for command in radio_cmd {
        match command {
            RadioCommand::Scan {
                client,
                start,
                stop,
                address,
                payload,
            } => {
                let res = scan(&mut crazyradio, start, stop, address, payload);
                // Ignore the error if the client has dropped since it did the request
                let _ = client.send(res);
            }
            RadioCommand::SendPacket {
                client,
                channel,
                address,
                payload,
            } => {
                let res = send_packet(&mut crazyradio, channel, address, payload);
                // Ignore the error if the client has dropped since it did the request
                let _ = client.send(res);
            }
            RadioCommand::SendPacketNoAck {
                client,
                channel,
                address,
                payload,
            } => {
                let res = send_packet_no_ack(&mut crazyradio, channel, address, payload);
                // Ignore the error if the client has dropped since it did the request
                let _ = client.send(res);
            }
        }
    }
}
