use crazyradio::{Channel, Crazyradio, SharedCrazyradio};

#[tokio::main]
async fn main() -> Result<(), crazyradio::Error> {
    let radio = Crazyradio::open_first_async().await?;
    let cr = SharedCrazyradio::new(radio);

    let channel = Channel::from_number(78).unwrap();
    let address = [0xff, 0xe7, 0xe7, 0xe7, 0xe7];
    let payload = [0x8f, 0x07, 0x00, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x40, 0x40];

    // send a takeoff command via broadcast
    cr.send_packet_no_ack_async(channel, address, payload.to_vec()).await?;

    Ok(())
}
