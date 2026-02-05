# Crazyradio Rust driver [![Latest version](https://img.shields.io/crates/v/crazyradio.svg)](https://crates.io/crates/crazyradio) [![Documentation](https://docs.rs/crazyradio/badge.svg)](https://docs.rs/crazyradio) [![tests](https://github.com/ataffanel/crazyradio-rs/workflows/tests/badge.svg)](https://github.com/ataffanel/crazyradio-rs/actions)

Crazyradio USB dongle driver for Rust.

This crate implements low level support for the Crazyradio PA USB dongle.
It implements the protocol documented in the [Crazyradio documentation](https://www.bitcraze.io/documentation/repository/crazyradio-firmware/master/functional-areas/usb_radio_protocol/).
It uses the rusb crates to access the USB device.

[Crazyradio](https://www.bitcraze.io/products/crazyradio-pa/) is a 2.4GHz USB
radio dongle based on the Nordic Semiconductor nRF24LU1 radio chip.
It is mainly intended to be used to control and communicate with the
Crazyflie nano quadcopter.

## Usage

The crates exposes a ```Crazyradio``` struct that can be used to open a
Crazyradio dongle, configure it, sent packet and receive ack with it. See the
[Crazyradio struct documentation](https://docs.rs/crazyradio) for an example.

To run the examples use, e.g.,:

```
cargo run --features async,shared_radio --example async_scan
cargo run --example console
```

## Shared and async radio

The feature `shared_radio` enables the `SharedCrazyradio` struct that
can be used to share a radio dongle between threads.

The feature `async` enables async functions in the `SharedRadio` struct as well
as to create the `Crazyradio` struct.

The feature `wireshark` enables packet capturing to Wireshark.

## Serde support

To enable Serde support for serializing and deserializing ```Channels```, enable the feature "serde_support".
