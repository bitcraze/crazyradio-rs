# Crazyradio Rust driver

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
