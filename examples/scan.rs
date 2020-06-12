use crazyradio::{Channel, Crazyradio};

fn main() -> Result<(), crazyradio::Error> {
    let mut cr = Crazyradio::open_first()?;

    println!("Opened Crazyradio with serial number: {}", cr.serial()?);

    println!("Scanning channels from 0 to 125 ...");
    let result = cr.scan_channels(
        Channel::from_number(0).unwrap(),
        Channel::from_number(125).unwrap(),
        &[0xff],
    )?;
    println!("Found {} Crazyflies:", result.len());
    for channel in result {
        println!("  {:?}", channel)
    }

    Ok(())
}
