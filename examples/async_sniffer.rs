use crazyradio::{Channel, Crazyradio, Datarate};

#[tokio::main]
async fn main() -> Result<(), crazyradio::Error> {
    // Open the second Crazyradio (index 1), assumes the first one is used by the client
    let mut cr = Crazyradio::open_nth_async(1).await?;

    // Configure radio parameters before entering sniffer mode
    cr.set_channel(Channel::from_number(70)?)?;
    cr.set_datarate(Datarate::Dr2M)?;
    cr.set_address(&[0xe7, 0xe7, 0xe7, 0xe7, 0xe7])?;

    println!("Entering async sniffer mode on channel 80, 2Mbps ...");
    let (receiver, sender) = cr.enter_sniffer_mode_async().await?;

    // Send a broadcast packet (no-ack) in sniffer mode
    sender.send_broadcast(&[0xff]).await?;
    println!("Sent broadcast packet");

    loop {
        match receiver.recv().await {
            Some(Ok(pkt)) => {
                println!(
                    "pipe:{} rssi:-{}dBm ts:{}us len:{} data:{:02x?}",
                    pkt.pipe,
                    pkt.rssi_dbm,
                    pkt.timestamp_us,
                    pkt.payload.len(),
                    &pkt.payload,
                );
            }
            Some(Err(e)) => {
                eprintln!("Error: {:?}", e);
                break;
            }
            None => {
                println!("Sniffer session ended");
                break;
            }
        }
    }

    // To recover the radio: receiver.close().await?
    // (not shown here since we loop forever)

    Ok(())
}
