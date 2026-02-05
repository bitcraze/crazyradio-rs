//! Wireshark packet capture support for Crazyradio
//!
//! This module provides a callback mechanism for capturing packets
//! sent and received via the Crazyradio.

use std::sync::OnceLock;

/// Direction: transmit (to device)
pub const DIRECTION_TX: u8 = 0;
/// Direction: receive (from device)
pub const DIRECTION_RX: u8 = 1;

/// Captured packet event data
#[derive(Debug, Clone)]
pub struct CaptureEvent<'a> {
    /// Packet direction: [`DIRECTION_TX`] or [`DIRECTION_RX`]
    pub direction: u8,
    /// Radio channel (0-125)
    pub channel: u8,
    /// 5-byte radio address
    pub address: &'a [u8; 5],
    /// Serial number of the radio device
    pub serial: &'a str,
    /// Packet payload data
    pub data: &'a [u8],
}

/// Packet capture callback type
pub type CaptureCallback = Box<dyn Fn(CaptureEvent<'_>) + Send + Sync>;

/// Global capture callback (set once at initialization)
static CAPTURE_CALLBACK: OnceLock<CaptureCallback> = OnceLock::new();

/// Set the packet capture callback
///
/// This should be called once at initialization to enable packet capture.
/// Subsequent calls will be silently ignored.
pub fn set_callback(callback: CaptureCallback) {
    let _ = CAPTURE_CALLBACK.set(callback);
}

/// Send a packet to the capture callback (if set)
pub(crate) fn capture_packet(direction: u8, channel: u8, address: &[u8; 5], serial: &str, data: &[u8]) {
    if let Some(callback) = CAPTURE_CALLBACK.get() {
        callback(CaptureEvent {
            direction,
            channel,
            address,
            serial,
            data,
        });
    }
}
