use crazyradio::{Channel, Crazyradio, Datarate};

fn main() -> Result<(), crazyradio::Error> {
    let mut cr = Crazyradio::open_first()?;

    cr.set_datarate(Datarate::Dr2M)?;

    println!("Scanning for Crazyflies ...");
    let channels = cr.scan_channels(
        Channel::from_number(0).unwrap(),
        Channel::from_number(125).unwrap(),
        &[0xff],
    )?;

    if channels.is_empty() {
        println!("No Crazyflie found!");
        return Ok(());
    }

    println!(
        "{} Crazyflie(s) found, connecting to {:?}.",
        channels.len(),
        channels[0]
    );

    cr.set_channel(channels[0])?;

    println!("Sending packets and displaying RSSI (press Ctrl+C to stop):");
    println!("Channel | RSSI (dBm) | Retries | Ack payload length");
    println!("--------|------------|---------|-------------------");

    let mut ack_data = [0u8; 32];
    loop {
        match cr.send_packet(&[0xff], &mut ack_data) {
            Ok(ack) => {
                let rssi_str = match ack.rssi_dbm {
                    // rssi_dbm is inverted: -60dBm is encoded as 60
                    Some(raw) => format!("-{} dBm", raw),
                    None => "N/A (requires CR2 fw >= 5.3)".to_string(),
                };
                println!(
                    "{:?}   | {:26} | {:7} | {}",
                    channels[0], rssi_str, ack.retry, ack.length
                );
            }
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}
