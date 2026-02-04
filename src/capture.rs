//! Wireshark packet capture support for Crazyradio
//!
//! This module provides a callback mechanism for capturing packets
//! sent and received via the Crazyradio.

use std::sync::{Mutex, OnceLock};

/// Direction: transmit (to device)
pub const DIRECTION_TX: u8 = 0;
/// Direction: receive (from device)
pub const DIRECTION_RX: u8 = 1;

/// Packet capture callback type
///
/// Arguments: (direction, channel, address, radio_index, data)
pub type CaptureCallback = Box<dyn Fn(u8, u8, &[u8; 5], u8, &[u8]) + Send + Sync>;

/// Global capture callback
static CAPTURE_CALLBACK: OnceLock<Mutex<Option<CaptureCallback>>> = OnceLock::new();

/// Set the packet capture callback
///
/// This should be called once at initialization to enable packet capture.
/// The callback will be invoked for every packet sent or received.
pub fn set_callback(callback: CaptureCallback) {
    let cb = CAPTURE_CALLBACK.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cb.lock() {
        *guard = Some(callback);
    }
}

/// Clear the packet capture callback
pub fn clear_callback() {
    if let Some(cb) = CAPTURE_CALLBACK.get() {
        if let Ok(mut guard) = cb.lock() {
            *guard = None;
        }
    }
}

/// Send a packet to the capture callback (if set)
pub(crate) fn capture_packet(direction: u8, channel: u8, address: &[u8; 5], radio_index: u8, data: &[u8]) {
    if let Some(cb) = CAPTURE_CALLBACK.get() {
        if let Ok(guard) = cb.lock() {
            if let Some(ref callback) = *guard {
                callback(direction, channel, address, radio_index, data);
            }
        }
    }
}
