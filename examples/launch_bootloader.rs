use crazyradio;

fn main() -> Result<(), crazyradio::Error>{
    let cr = crazyradio::Crazyradio::new()?;

    cr.launch_bootloader()?;

    Ok(())
}