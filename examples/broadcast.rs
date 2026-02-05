use crazyradio::{Channel, Crazyradio, Datarate};

fn main() -> Result<(), crazyradio::Error> {
    let mut cr = Crazyradio::open_first()?;

    cr.set_datarate(Datarate::Dr2M)?;
    cr.set_channel(Channel::from_number(78).unwrap())?;
    cr.set_address(&[0xff, 0xe7, 0xe7, 0xe7, 0xe7])?;
    cr.set_ack_enable(false)?;

    // send a takeoff command via broadcast
    cr.send_packet_no_ack(&[
        0x8f, 0x07, 0x00, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x40,
        0x40,
    ])?;

    Ok(())
}
