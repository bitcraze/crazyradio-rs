#![cfg(feature = "async")]
#![cfg_attr(docsrs, doc(cfg(feature = "async")))]

use std::sync::Arc;
use std::time::Duration;

use crate::{Crazyradio, Error, Result, UsbCommand};

/// A packet received in async sniffer mode, with owned payload.
#[derive(Debug, Clone)]
pub struct ReceivedSnifferPacket {
    /// RSSI in dBm (negative, e.g. -60 means -60 dBm)
    pub rssi_dbm: i16,
    /// Pipe index the packet was received on (0 or 1)
    pub pipe: u8,
    /// Timestamp in microseconds (wraps every ~71 minutes)
    pub timestamp_us: u32,
    /// Packet payload
    pub payload: Vec<u8>,
}

/// Receives sniffed packets from the radio.
///
/// This handle is **not** `Clone` — only one receiver is allowed.
/// Use [`close`](SnifferReceiver::close) to exit sniffer mode and recover the [`Crazyradio`].
pub struct SnifferReceiver {
    packet_rx: Option<flume::Receiver<Result<ReceivedSnifferPacket>>>,
    close_tx: Option<flume::Sender<()>>,
    radio_rx: Option<flume::Receiver<Result<Crazyradio>>>,
}

/// Sends broadcast packets and queries drop count while in sniffer mode.
///
/// This handle **can be cloned** — multiple senders are allowed.
#[derive(Clone)]
pub struct SnifferSender {
    device_handle: Arc<rusb::DeviceHandle<rusb::GlobalContext>>,
    #[cfg(feature = "packet_capture")]
    channel: u8,
    #[cfg(feature = "packet_capture")]
    address: [u8; 5],
    #[cfg(feature = "packet_capture")]
    serial: String,
}

impl SnifferReceiver {
    /// Receive the next sniffed packet.
    ///
    /// Returns `None` when the sniffer session has been closed (e.g. the RX
    /// thread exited due to a USB error or after [`close`](Self::close) was called).
    pub async fn recv(&self) -> Option<Result<ReceivedSnifferPacket>> {
        let rx = self.packet_rx.as_ref()?;
        match rx.recv_async().await {
            Ok(result) => Some(result),
            Err(_) => None, // channel disconnected
        }
    }

    /// Close the sniffer session and recover the [`Crazyradio`].
    ///
    /// Signals the RX thread to stop, waits for it to exit sniffer mode,
    /// and returns the radio for normal use.
    pub async fn close(mut self) -> Result<Crazyradio> {
        // Signal the RX thread to stop
        drop(self.close_tx.take());
        // Drain the packet channel so the RX thread isn't blocked trying to send
        if let Some(packet_rx) = self.packet_rx.take() {
            drop(packet_rx);
        }
        // Wait for the radio to be returned
        if let Some(radio_rx) = self.radio_rx.take() {
            match radio_rx.recv_async().await {
                Ok(result) => result,
                Err(_) => Err(Error::SnifferSessionClosed),
            }
        } else {
            Err(Error::SnifferSessionClosed)
        }
    }
}

impl SnifferSender {
    /// Send a broadcast (no-ack) packet while in sniffer mode.
    ///
    /// The packet is sent using the channel, datarate, and pipe-0 address
    /// that were configured before entering sniffer mode. The radio briefly
    /// leaves RX mode during TX (~1 ms).
    pub async fn send_broadcast(&self, data: &[u8]) -> Result<()> {
        if data.is_empty() || data.len() > 32 {
            return Err(Error::InvalidArgument);
        }

        let handle = self.device_handle.clone();
        let data = data.to_vec();

        #[cfg(feature = "packet_capture")]
        let (channel, address, serial) = (self.channel, self.address, self.serial.clone());

        let (tx, rx) = flume::bounded(1);
        std::thread::spawn(move || {
            #[cfg(feature = "packet_capture")]
            crate::capture::capture_packet(
                crate::capture::DIRECTION_TX,
                channel,
                &address,
                &serial,
                &data,
            );

            let result = handle
                .write_bulk(0x01, &data, Duration::from_secs(1))
                .map(|_| ())
                .map_err(Error::from);
            let _ = tx.send(result);
        });

        rx.recv_async().await.unwrap()
    }

    /// Get the number of packets dropped due to queue overflow since sniffer
    /// mode was last entered.
    pub async fn get_drop_count(&self) -> Result<u32> {
        let handle = self.device_handle.clone();

        let (tx, rx) = flume::bounded(1);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4];
            let result = handle
                .read_control(
                    0xC0,
                    UsbCommand::GetSnifferDropCount as u8,
                    0,
                    0,
                    &mut buf,
                    Duration::from_secs(1),
                )
                .map(|_| u32::from_le_bytes(buf))
                .map_err(Error::from);
            let _ = tx.send(result);
        });

        rx.recv_async().await.unwrap()
    }
}

/// RX loop that runs in a dedicated thread.
fn sniffer_rx_loop(
    mut cr: Crazyradio,
    packet_tx: flume::Sender<Result<ReceivedSnifferPacket>>,
    close_rx: flume::Receiver<()>,
    radio_tx: flume::Sender<Result<Crazyradio>>,
) {
    const RX_TIMEOUT: Duration = Duration::from_secs(1);

    loop {
        // Check if we should stop (close signal or packet channel disconnected)
        if close_rx.try_recv().is_ok() || packet_tx.is_disconnected() {
            break;
        }

        let mut payload_buf = [0u8; 32];
        match cr.receive_sniffer_packet(&mut payload_buf, RX_TIMEOUT) {
            Ok(Some(pkt)) => {
                let received = ReceivedSnifferPacket {
                    rssi_dbm: pkt.rssi_dbm,
                    pipe: pkt.pipe,
                    timestamp_us: pkt.timestamp_us,
                    payload: payload_buf[..pkt.length].to_vec(),
                };
                if packet_tx.send(Ok(received)).is_err() {
                    break; // receiver dropped
                }
            }
            Ok(None) => {
                // Timeout, loop around and check close signal
            }
            Err(e) => {
                // Send the error and stop
                let _ = packet_tx.send(Err(e));
                break;
            }
        }
    }

    // Exit sniffer mode and return the radio
    let result = cr.exit_sniffer_mode().map(|_| cr);
    let _ = radio_tx.send(result);
}

/// Enter async sniffer mode. Called by `Crazyradio::enter_sniffer_mode_async`.
pub(crate) async fn enter_sniffer_mode_async(
    mut cr: Crazyradio,
) -> Result<(SnifferReceiver, SnifferSender)> {
    // Capture state needed for the sender before moving cr into the thread
    let device_handle = cr.device_handle.clone();

    #[cfg(feature = "packet_capture")]
    let channel: u8 = cr.channel.into();
    #[cfg(feature = "packet_capture")]
    let address = cr.address;
    #[cfg(feature = "packet_capture")]
    let serial = cr.serial.clone();

    // Enter sniffer mode (blocking USB call) on a spawned thread
    let (setup_tx, setup_rx) = flume::bounded(1);
    std::thread::spawn(move || {
        match cr.enter_sniffer_mode() {
            Ok(()) => {
                let _ = setup_tx.send(Ok(cr));
            }
            Err(e) => {
                let _ = setup_tx.send(Err(e));
            }
        }
    });

    let cr = setup_rx.recv_async().await.unwrap()?;

    // Now cr is in sniffer mode. Set up channels and spawn RX thread.
    let (packet_tx, packet_rx) = flume::bounded(128);
    let (close_tx, close_rx) = flume::bounded(1);
    let (radio_tx, radio_rx) = flume::bounded(1);

    // We need to move cr into the RX thread. But first, ensure device_handle
    // Arc is shared. cr.device_handle is already an Arc so the sender's clone
    // keeps it alive even after cr moves.

    std::thread::spawn(move || {
        sniffer_rx_loop(cr, packet_tx, close_rx, radio_tx);
    });

    let receiver = SnifferReceiver {
        packet_rx: Some(packet_rx),
        close_tx: Some(close_tx),
        radio_rx: Some(radio_rx),
    };

    let sender = SnifferSender {
        device_handle,
        #[cfg(feature = "packet_capture")]
        channel,
        #[cfg(feature = "packet_capture")]
        address,
        #[cfg(feature = "packet_capture")]
        serial,
    };

    Ok((receiver, sender))
}
