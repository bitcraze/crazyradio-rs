use crazyradio::{Channel, Crazyradio, Datarate};
use std::time::Duration;

fn main() -> Result<(), crazyradio::Error> {
    // Open Crazyradio
    let mut cr = Crazyradio::open_first()?;

    // Configure radio parameters before entering sniffer mode
    cr.set_channel(Channel::from_number(80)?)?;
    cr.set_datarate(Datarate::Dr2M)?;
    cr.set_address(&[0xe7, 0xe7, 0xe7, 0xe7, 0xe7])?;
    cr.set_sniffer_address(1, &[0xff, 0xe7, 0xe7, 0xe7, 0xe7])?;

    println!("Entering sniffer mode on channel 80, 2Mbps ...");
    cr.enter_sniffer_mode()?;

    // Send a broadcast packet (no-ack) in sniffer mode.
    // This transmits using the current channel, datarate, and pipe-0 address.
    cr.send_sniffer_broadcast(&[0xe7, 0xe7, 0xe7, 0xe7, 0xe7], &[0xff])?;
    println!("Sent broadcast packet");

    let mut payload = [0u8; 63];
    loop {
        match cr.receive_sniffer_packet(&mut payload, Duration::from_secs(1))? {
            Some(pkt) => {
                println!(
                    "pipe:{} rssi:{}dBm ts:{}us len:{} data:{:02x?}",
                    pkt.pipe,
                    pkt.rssi_dbm,
                    pkt.timestamp_us,
                    pkt.length,
                    &payload[..pkt.length],
                );
            }
            None => {
                let drops = cr.get_sniffer_drop_count()?;
                println!("(timeout, drops: {})", drops);
            }
        }
    }
}
