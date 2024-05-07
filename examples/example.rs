use nhope::Engine;
use std::error::Error;
use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::WindowBuilder,
};

const APP_NAME: &str = "Nhope Engine";
const WIN_WIDTH: u32 = 1920;
const WIN_HEIGHT: u32 = 1080;

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title(APP_NAME)
        .with_inner_size(winit::dpi::LogicalSize::new(
            f64::from(WIN_WIDTH),
            f64::from(WIN_HEIGHT),
        ))
        .build(&event_loop)
        .unwrap();

    let mut engine = Engine::new(&window)?;
    engine.start_update_thread();

    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run(|event, elwt| match event {
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
            } => elwt.exit(),
            WindowEvent::Resized(size) => engine.recreate_swapchain(size),
            WindowEvent::RedrawRequested => {
                engine.render();
                profiling::finish_frame!();
            }
            _ => {}
        },
        Event::AboutToWait => {
            engine.render();
            profiling::finish_frame!();
        }
        _ => (),
    })?;

    Ok(())
}
