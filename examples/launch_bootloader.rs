use crazyradio;

fn main() -> Result<(), crazyradio::Error>{
    let cr = crazyradio::Crazyradio::open_first()?;

    cr.launch_bootloader()?;

    Ok(())
}