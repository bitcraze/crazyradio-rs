use crazyradio::{Crazyradio, Channel};

fn main() -> Result<(), crazyradio::Error> {
    let mut cr = Crazyradio::new()?;

    println!("Scanning all channels using hardware scann ...");
    let result = cr.scan_channels(Channel::new(0).unwrap(),
                                  Channel::new(125).unwrap(),
                                  &[0xff])?;
    println!("Found {} Crazyflies:", result.len());
    for channel in result {
        println!("  {:?}", channel)
    }

    println!("\nScanning all channels using software scann ...");
    let mut ack_data = [0u8; 32];
    let mut result: Vec<Channel> = vec![];
    for ch in 0..126 {
        let channel = Channel::new(ch).unwrap();
        cr.set_channel(channel)?;
        let n_received = cr.send_packet(&[0xff], &mut ack_data)?;
        if n_received > 0 {
            result.push(channel);
        }
    }
    println!("Found {} Crazyflies:", result.len());
    for channel in result {
        println!("  {:?}", channel)
    }

    Ok(())
}