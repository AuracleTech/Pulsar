use nhope::Engine;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut base = Engine::new(1920, 1080)?;
    base.looping();
    Ok(())
}
