// #![cfg(windows)]
// #![windows_subsystem = "windows"]

const APP_NAME: &str = "Nhope Engine";
const WIN_WIDTH: u32 = 1920;
const WIN_HEIGHT: u32 = 1080;
use std::error::Error;

use nhope::{window::Window, Engine};

fn main() -> Result<(), Box<dyn Error>> {
    let window = Window::create_main(APP_NAME, APP_NAME, WIN_WIDTH, WIN_HEIGHT)?;
    let mut engine = Engine::new(&window)?;
    window.show();

    std::thread::spawn(move || loop {
        engine.render();
    });
    window.run();

    Ok(())
}
