use crazyradio::{Crazyradio, Channel, Datarate};
use std::str;

fn main() -> Result<(), crazyradio::Error>{
    let mut cr = Crazyradio::open_first()?;

    cr.set_datarate(Datarate::Dr2M)?;

    println!("Scanning for Crazyflies ...");
    let channels = cr.scan_channels(Channel::from_number(0).unwrap(),
                                    Channel::from_number(125).unwrap(),
                                    &[0xff])?;
    if channels.len() > 0 {
        println!("{} Crazyflies found, connecting {:?}.", channels.len(), channels[0]);

        cr.set_channel(channels[0])?;
    
        println!("Fetching and displaying up to 100 console packets:");
        println!("==================================================");
        let mut ack_data = [0u8; 32];
        for _i in 1..100 {
            if let Ok(received_len) = cr.send_packet(&[0xff], &mut ack_data) {
                if received_len > 0 && ack_data[0] == 0 {
                    print!("{}", str::from_utf8(&ack_data[1..received_len]).unwrap());
                }
            }
        }
    } else {
        println!("No Crazyflie found!");
    }
    

    Ok(())
}