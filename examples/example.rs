use pulsar::app::{Application, UserEvent};
use std::error::Error;
use winit::event_loop::EventLoop;

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut app = Application::new(&event_loop)?;
    event_loop.run_app(&mut app).map_err(Into::into)
}
