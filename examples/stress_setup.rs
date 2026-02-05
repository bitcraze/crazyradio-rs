use crazyradio::{Channel, Crazyradio};
use indicatif::{HumanCount, ProgressBar};

fn main() -> Result<(), crazyradio::Error> {
    let mut cr = Crazyradio::open_first()?;
    cr.set_channel(Channel::from_number(42)?)?;
    cr.set_datarate(crazyradio::Datarate::Dr2M)?;
    cr.set_address(&[0xe7, 0xe7, 0xe7, 0xe7, 0x42])?;
    // cr.set_arc(0)?;

    let pb = ProgressBar::new_spinner();
    pb.set_message("Running stress test...");

    println!("Opened Crazyradio with serial number: {}", cr.serial()?);

    let mut i = 0;
    loop {
        i += 1;
        for j in 0..100usize {
            // cr.set_channel(Channel::from_number(0)?)?;
            // cr.set_ack_enable(j.is_multiple_of(2))?;
            if j.is_multiple_of(2) {
                cr.set_address(&[0xff, 0xe7, 0xe7, 0xe7, 0xff])?;
                cr.set_ack_enable(false)?;
                cr.send_packet_no_ack(&[0xff])?;
            } else {
                cr.set_address(&[0xe7, 0xe7, 0xe7, 0xe7, 0x42])?;
                cr.set_ack_enable(true)?;
                let mut ack_data = [0u8; 32];
                cr.send_packet(&[0xff], &mut ack_data)?;
            }
        }
        pb.set_message(format!("Iterations: {} ({} loops)", i, HumanCount(i * 100)));
        pb.tick();
    }
}
