use nhope::Engine;
use std::error::Error;

use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::ControlFlow,
    keyboard::{Key, NamedKey},
};

fn main() -> Result<(), Box<dyn Error>> {
    let (engine, event_loop) = Engine::new(1920, 1080)?;
    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Poll);
        match event {
            Event::WindowEvent {
                event: window_event,
                ..
            } => match window_event {
                WindowEvent::CloseRequested
                | WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            state: ElementState::Pressed,
                            logical_key: Key::Named(NamedKey::Escape),
                            ..
                        },
                    ..
                } => {
                    elwt.exit();
                }
                WindowEvent::Resized(_new_size) => {
                    // IMPLEMENT
                }
                _ => (),
            },
            Event::AboutToWait => engine.render(),
            _ => (),
        }
    })?;

    Ok(())
}
