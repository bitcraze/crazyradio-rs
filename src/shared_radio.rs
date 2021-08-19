use crate::Result;
use crate::{Ack, Channel, Crazyradio};
use flume::{bounded, unbounded, Receiver, Sender};

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
/// let radio_thread = crazyradio::SharedCrazyradio::new(radio);
///
/// let radio_thread2 = radio_thread.clone();
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
    scan_res_send: Sender<Result<ScanResult>>,
    scan_res: Receiver<Result<ScanResult>>,
}

impl SharedCrazyradio {
    pub fn new(radio: Crazyradio) -> Self {
        let (radio_command, radio_command_recv) = unbounded();

        std::thread::spawn(move || {
            radio_loop(radio, radio_command_recv);
        });

        let (send_packet_res_send, send_packet_res) = bounded(1);
        let (scan_res_send, scan_res) = bounded(1);

        SharedCrazyradio {
            radio_command,
            send_packet_res_send,
            send_packet_res,
            scan_res_send,
            scan_res,
        }
    }

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

    pub async fn scan_async(
        &self,
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

    pub fn send_packet(
        &self,
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
                power_detector: false,
                retry: 0,
            },
            result.payload,
        ))
    }

    pub async fn send_packet_async(
        &self,
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
                power_detector: false,
                retry: 0,
            },
            result.payload,
        ))
    }
}

impl Clone for SharedCrazyradio {
    fn clone(&self) -> Self {
        // Create new pair of return channels
        let (send_packet_res_send, send_packet_res) = bounded(1);
        let (scan_res_send, scan_res) = bounded(1);

        // The command channel is clonned
        let radio_command = self.radio_command.clone();

        SharedCrazyradio {
            radio_command,
            send_packet_res_send,
            send_packet_res,
            scan_res_send,
            scan_res,
        }
    }
}

enum RadioCommand {
    SendPacket {
        client: Sender<Result<SendPacketResult>>,
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

    let ack = crazyradio.send_packet(&payload, &mut ack_data)?;
    ack_data.resize(ack.length, 0);

    Ok(SendPacketResult {
        acked: ack.received,
        payload: ack_data,
    })
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
        }
    }
}
