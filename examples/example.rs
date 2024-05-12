use nhope::app::{Application, UserEvent};
use nhope::Engine;
use std::error::Error;
use winit::event_loop::EventLoop;

// const APP_NAME: &str = "Nhope Engine";
const WIN_WIDTH: u32 = 1920;
const WIN_HEIGHT: u32 = 1080;

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(web_platform)]
    console_error_panic_hook::set_once();

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let _event_loop_proxy = event_loop.create_proxy();

    let mut app = Application::new(&event_loop);

    // Wire the user event from another thread.
    // #[cfg(not(web_platform))]
    // std::thread::spawn(move || {
    //     // Wake up the `event_loop` once every second and dispatch a custom event
    //     // from a different thread.
    //     // info!("Starting to send user event every second");
    //     loop {
    //         // let _ = _event_loop_proxy.send_event(UserEvent::WakeUp);
    //         // std::thread::sleep(std::time::Duration::from_secs(1));
    //     }
    // });

    event_loop.run_app(&mut app).map_err(Into::into)

    // let event_loop = EventLoop::new()?;
    // let window = WindowBuilder::new()
    //     .with_title(APP_NAME)
    //     .with_inner_size(winit::dpi::LogicalSize::new(
    //         f64::from(WIN_WIDTH),
    //         f64::from(WIN_HEIGHT),
    //     ))
    //     .build(&event_loop)
    //     .unwrap();

    // let mut engine = Engine::new(&window)?;
    // engine.start_update_thread();

    // event_loop.set_control_flow(ControlFlow::Poll);
    // event_loop.run(|event, elwt| match event {
    //     Event::WindowEvent {
    //         event: window_event,
    //         ..
    //     } => match window_event {
    //         WindowEvent::CloseRequested
    //         | WindowEvent::KeyboardInput {
    //             event:
    //                 KeyEvent {
    //                     state: ElementState::Pressed,
    //                     logical_key: Key::Named(NamedKey::Escape),
    //                     ..
    //                 },
    //             ..
    //         } => elwt.exit(),
    //         WindowEvent::Resized(size) => engine.recreate_swapchain(size),
    //         WindowEvent::RedrawRequested => {
    //             engine.render();
    //             profiling::finish_frame!();
    //         }
    //         _ => {}
    //     },
    //     Event::AboutToWait => {
    //         engine.render();
    //         profiling::finish_frame!();
    //     }
    //     _ => (),
    // })?;

    // Ok(())
}
