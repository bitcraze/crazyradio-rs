use crazyradio::{Channel, Crazyradio, SharedCrazyradio};

#[async_std::main]
async fn main() -> Result<(), crazyradio::Error> {
    let cr = SharedCrazyradio::new(Crazyradio::open_first()?);

    println!("Scanning channels from 0 to 125 ...");
    let result = cr
        .scan_async(
            Channel::from_number(0).unwrap(),
            Channel::from_number(125).unwrap(),
            [0xe7; 5],
            vec![0xff],
        )
        .await?;
    println!("Found {} Crazyflies:", result.len());
    for channel in result {
        println!("  {:?}", channel)
    }

    Ok(())
}
