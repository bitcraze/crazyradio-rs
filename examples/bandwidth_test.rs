use crazyradio::{Channel, Crazyradio, Datarate};
use std::time::Instant;

const N_PACKETS: usize = 10000;

fn main() -> Result<(), crazyradio::Error> {
    let mut cr = Crazyradio::open_first()?;

    cr.set_datarate(Datarate::Dr2M)?;
    cr.set_channel(Channel::from_number(42)?)?;
    cr.set_address(&[0xe7, 0xe7, 0xe7, 0xe7, 0x42])?;
    cr.set_arc(0)?;

    cr.set_packet_loss_simulation(0, 10)?;

    // Setup packet
    let crtp_port = 15;
    let crtp_channel = 0;
    let header = ((crtp_port & 0x0F) << 4) | (crtp_channel & 0x0F);
    let payload_size = 28;
    let packet = vec![header as u8; payload_size + 1]; // +1 for header byte

    let mut n_ack = 0;
    let mut n_syslink = 0;
    let start = Instant::now();

    for _ in 0..N_PACKETS {
        let mut ack_data = [0u8; 32];
        let ack = cr.send_packet(&packet, &mut ack_data)?;
        if ack.received {
            n_ack += 1;

            if ack_data.len() > 2 && ack_data[0] & 0xFC == 0xF0 {
                n_syslink += 1;
            }
        }

        // sleep(Duration::from_micros(100)); // Small delay to avoid overwhelming the radio
    }

    let duration = start.elapsed();
    let seconds = duration.as_secs_f64();
    let pps = N_PACKETS as f64 / seconds;

    println!("Sent {} packets in {:.2} seconds", N_PACKETS, seconds);
    println!("Throughput: {:.2} packets/second", pps);
    println!(
        "Packet success rate: {:.2}%",
        (n_ack as f64 / N_PACKETS as f64) * 100.0
    );
    println!(
        "Syslink packet rate: {:.2}% ({} pk/s)",
        (n_syslink as f64 / N_PACKETS as f64) * 100.0,
        (n_syslink as f64 / seconds)
    );

    Ok(())
}
