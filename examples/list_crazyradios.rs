fn main() -> Result<(), crazyradio::Error> {
    let serials = crazyradio::Crazyradio::list_serials()?;

    println!("{} Crazyradio found:", serials.len());

    for serial in serials.iter() {
        println!("  - {}", serial);
    }

    Ok(())
}
