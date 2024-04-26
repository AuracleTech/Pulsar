use nhope::ExampleBase;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut base = ExampleBase::new(1920, 1080)?;
    base.looping();
    Ok(())
}
