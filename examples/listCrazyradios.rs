use crazyradio;

fn main() -> Result<(), crazyradio::Error> {

    crazyradio::Crazyradio::list_serials()?;

    Ok(())
}