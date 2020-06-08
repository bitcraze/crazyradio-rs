use crazyradio::{Crazyradio, Channel};

fn main() -> Result<(), crazyradio::Error> {
    let mut cr = Crazyradio::new()?;

    println!("Scanning channels from 0 to 125 ...");
    let result = cr.scan_channels(Channel::new(0).unwrap(),
                                  Channel::new(125).unwrap(),
                                  &[0xff])?;
    println!("Found {} Crazyflies:", result.len());
    for channel in result {
        println!("  {:?}", channel)
    }

    Ok(())
}