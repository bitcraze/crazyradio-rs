use crazyradio::{Channel, Crazyradio, SharedCrazyradio};

#[tokio::main]
async fn main() -> Result<(), crazyradio::Error> {
    let radio = Crazyradio::open_first_async().await?;
    let mut cr = SharedCrazyradio::new(radio);

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
