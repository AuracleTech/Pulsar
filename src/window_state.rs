use crate::{
    app::Application,
    input_manager::EventStates,
    vulkan::{graphics::AAAGraphics, surface::AAASurface, AAABase},
};
use cursor_icon::CursorIcon;
use log::info;
use std::{
    error::Error,
    mem,
    sync::{Arc, Mutex},
    thread,
};
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    keyboard::ModifiersState,
    window::{Cursor, CursorGrabMode, CustomCursor, Fullscreen, ResizeDirection, Theme, Window},
};

/// The amount of points to around the window for drag resize direction calculations.
const BORDER_SIZE: f64 = 20.;

const CURSORS: &[CursorIcon] = &[
    CursorIcon::Default,
    CursorIcon::Crosshair,
    CursorIcon::Pointer,
    CursorIcon::Move,
    CursorIcon::Text,
    CursorIcon::Wait,
    CursorIcon::Help,
    CursorIcon::Progress,
    CursorIcon::NotAllowed,
    CursorIcon::ContextMenu,
    CursorIcon::Cell,
    CursorIcon::VerticalText,
    CursorIcon::Alias,
    CursorIcon::Copy,
    CursorIcon::NoDrop,
    CursorIcon::Grab,
    CursorIcon::Grabbing,
    CursorIcon::AllScroll,
    CursorIcon::ZoomIn,
    CursorIcon::ZoomOut,
    CursorIcon::EResize,
    CursorIcon::NResize,
    CursorIcon::NeResize,
    CursorIcon::NwResize,
    CursorIcon::SResize,
    CursorIcon::SeResize,
    CursorIcon::SwResize,
    CursorIcon::WResize,
    CursorIcon::EwResize,
    CursorIcon::NsResize,
    CursorIcon::NeswResize,
    CursorIcon::NwseResize,
    CursorIcon::ColResize,
    CursorIcon::RowResize,
];

pub struct WindowState {
    /// IME input.
    ime: bool,
    /// The actual winit Window.
    pub window: Arc<Window>,
    /// The window theme we're drawing with.
    theme: Theme,
    /// Cursor position over the window.
    cursor_position: Option<PhysicalPosition<f64>>,
    /// Window modifiers state.
    pub modifiers: ModifiersState,
    /// Occlusion state of the window.
    occluded: bool,
    /// Current cursor grab mode.
    cursor_grab: CursorGrabMode,
    /// The amount of zoom into window.
    pub zoom: f64,
    /// The amount of rotation of the window.
    pub rotated: f32,
    /// The amount of pan of the window.
    pub panned: PhysicalPosition<f32>,

    #[cfg(macos_platform)]
    option_as_alt: OptionAsAlt,

    // Cursor states.
    named_idx: usize,
    custom_idx: usize,
    cursor_hidden: bool,

    // Render
    // // TODO Remove pub
    pub renderer: Arc<AAABase>,
    pub surface: Arc<Mutex<AAASurface>>,

    pub graphics: Option<Arc<Mutex<AAAGraphics>>>,

    pub render_handle: Option<thread::JoinHandle<()>>,

    pub event_states: Arc<EventStates>,
}

impl WindowState {
    pub fn new(app: &Application, window: Window) -> Result<Self, Box<dyn Error>> {
        let window = Arc::new(window);

        let theme = window.theme().unwrap_or(Theme::Dark);
        info!("Theme: {theme:?}");
        let named_idx = 0;
        window.set_cursor(CURSORS[named_idx]);

        // Allow IME out of the box.
        let ime = true;
        window.set_ime_allowed(ime);

        let renderer = app.renderer.clone();

        let surface =
            crate::vulkan::surface::AAASurface::new(&renderer, &window, &app.physical_device_list)
                .unwrap();

        Ok(Self {
            #[cfg(macos_platform)]
            option_as_alt: window.option_as_alt(),
            custom_idx: app.custom_cursors.len() - 1,
            cursor_grab: CursorGrabMode::None,
            named_idx,
            window,
            theme,
            ime,
            cursor_position: Default::default(),
            cursor_hidden: Default::default(),
            modifiers: Default::default(),
            occluded: Default::default(),
            rotated: Default::default(),
            panned: Default::default(),
            zoom: Default::default(),
            renderer,
            surface: Arc::new(Mutex::new(surface)),
            render_handle: Default::default(),
            event_states: Default::default(),
            graphics: Default::default(),
        })
    }

    pub fn toggle_ime(&mut self) {
        self.ime = !self.ime;
        self.window.set_ime_allowed(self.ime);
        if let Some(position) = self.ime.then_some(self.cursor_position).flatten() {
            self.window
                .set_ime_cursor_area(position, PhysicalSize::new(20, 20));
        }
    }

    pub fn minimize(&mut self) {
        self.window.set_minimized(true);
    }

    pub fn cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        self.cursor_position = Some(position);
        if self.ime {
            self.window
                .set_ime_cursor_area(position, PhysicalSize::new(20, 20));
        }
    }

    pub fn cursor_left(&mut self) {
        self.cursor_position = None;
    }

    /// Toggle maximized.
    pub fn toggle_maximize(&self) {
        let maximized = self.window.is_maximized();
        self.window.set_maximized(!maximized);
    }

    /// Toggle window decorations.
    pub fn toggle_decorations(&self) {
        let decorated = self.window.is_decorated();
        self.window.set_decorations(!decorated);
    }

    /// Toggle window resizable state.
    pub fn toggle_resizable(&self) {
        let resizable = self.window.is_resizable();
        self.window.set_resizable(!resizable);
    }

    /// Toggle cursor visibility
    pub fn toggle_cursor_visibility(&mut self) {
        self.cursor_hidden = !self.cursor_hidden;
        self.window.set_cursor_visible(!self.cursor_hidden);
    }

    /// Toggle resize increments on a window.
    pub fn toggle_resize_increments(&mut self) {
        let new_increments = match self.window.resize_increments() {
            Some(_) => None,
            None => Some(LogicalSize::new(25.0, 25.0)),
        };
        // info!("Had increments: {}", new_increments.is_none());
        self.window.set_resize_increments(new_increments);
    }

    /// Toggle fullscreen.
    pub fn toggle_fullscreen(&self) {
        let fullscreen = if self.window.fullscreen().is_some() {
            None
        } else {
            Some(Fullscreen::Borderless(None))
        };

        self.window.set_fullscreen(fullscreen);
    }

    /// Cycle through the grab modes ignoring errors.
    pub fn cycle_cursor_grab(&mut self) {
        self.cursor_grab = match self.cursor_grab {
            // CursorGrabMode::Locked is unimplemented yet.
            CursorGrabMode::None => CursorGrabMode::Confined,
            CursorGrabMode::Confined => CursorGrabMode::None,
            _ => CursorGrabMode::None,
        };
        info!("Changing cursor grab mode to {:?}", self.cursor_grab);
        if let Err(err) = self.window.set_cursor_grab(self.cursor_grab) {
            panic!("Error setting cursor grab: {err}");
        }
    }

    #[cfg(macos_platform)]
    fn cycle_option_as_alt(&mut self) {
        self.option_as_alt = match self.option_as_alt {
            OptionAsAlt::None => OptionAsAlt::OnlyLeft,
            OptionAsAlt::OnlyLeft => OptionAsAlt::OnlyRight,
            OptionAsAlt::OnlyRight => OptionAsAlt::Both,
            OptionAsAlt::Both => OptionAsAlt::None,
        };
        // info!("Setting option as alt {:?}", self.option_as_alt);
        self.window.set_option_as_alt(self.option_as_alt);
    }

    /// Swap the window dimensions with `request_inner_size`.
    pub fn swap_dimensions(&mut self) {
        let old_inner_size = self.window.inner_size();
        let mut inner_size = old_inner_size;

        mem::swap(&mut inner_size.width, &mut inner_size.height);
        // info!("Requesting resize from {old_inner_size:?} to {inner_size:?}");

        if let Some(new_inner_size) = self.window.request_inner_size(inner_size) {
            if old_inner_size != new_inner_size {
                self.resize(new_inner_size)
            }
        } else {
            // info!("Request inner size is asynchronous");
        }
    }

    pub fn next_cursor(&mut self) {
        self.named_idx = (self.named_idx + 1) % CURSORS.len();
        // info!("Setting cursor to \"{:?}\"", CURSORS[self.named_idx]);
        self.window
            .set_cursor(Cursor::Icon(CURSORS[self.named_idx]));
    }

    pub fn next_custom_cursor(&mut self, custom_cursors: &[CustomCursor]) {
        self.custom_idx = (self.custom_idx + 1) % custom_cursors.len();
        let cursor = Cursor::Custom(custom_cursors[self.custom_idx].clone());
        self.window.set_cursor(cursor);
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        #[cfg(not(any(android_platform, ios_platform)))]
        {
            self.render_thread_close_join();

            let width = size.width;
            let height = size.height;

            let graphics_locked = self.graphics.clone().unwrap();
            let mut graphics = graphics_locked.lock().unwrap();
            graphics.recreate_swapchain(width, height);
            drop(graphics);

            self.spawn_render_thread_and_render();
        }
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn show_menu(&self) {
        if let Some(position) = self.cursor_position {
            self.window.show_window_menu(position);
        }
    }

    pub fn drag_window(&self) {
        if let Err(err) = self.window.drag_window() {
            info!("Error starting window drag: {err}");
        } else {
            info!("Dragging window Window={:?}", self.window.id());
        }
    }

    pub fn drag_resize_window(&self) {
        let position = match self.cursor_position {
            Some(position) => position,
            None => {
                info!("Drag-resize requires cursor to be inside the window");
                return;
            }
        };

        let win_size = self.window.inner_size();
        let border_size = BORDER_SIZE * self.window.scale_factor();

        let x_direction = if position.x < border_size {
            ResizeDirection::West
        } else if position.x > (win_size.width as f64 - border_size) {
            ResizeDirection::East
        } else {
            // Use arbitrary direction instead of None for simplicity.
            ResizeDirection::SouthEast
        };

        let y_direction = if position.y < border_size {
            ResizeDirection::North
        } else if position.y > (win_size.height as f64 - border_size) {
            ResizeDirection::South
        } else {
            // Use arbitrary direction instead of None for simplicity.
            ResizeDirection::SouthEast
        };

        let direction = match (x_direction, y_direction) {
            (ResizeDirection::West, ResizeDirection::North) => ResizeDirection::NorthWest,
            (ResizeDirection::West, ResizeDirection::South) => ResizeDirection::SouthWest,
            (ResizeDirection::West, _) => ResizeDirection::West,
            (ResizeDirection::East, ResizeDirection::North) => ResizeDirection::NorthEast,
            (ResizeDirection::East, ResizeDirection::South) => ResizeDirection::SouthEast,
            (ResizeDirection::East, _) => ResizeDirection::East,
            (_, ResizeDirection::South) => ResizeDirection::South,
            (_, ResizeDirection::North) => ResizeDirection::North,
            _ => return,
        };

        if let Err(err) = self.window.drag_resize_window(direction) {
            info!("Error starting window drag-resize: {err}");
        } else {
            info!("Drag-resizing window Window={:?}", self.window.id());
        }
    }

    /// Change window occlusion state.
    pub fn set_occluded(&mut self, occluded: bool) {
        self.occluded = occluded;
        if occluded {
            // TODO stop rendering
        }
    }

    pub fn create_renderer(&mut self) {
        let renderer = self.renderer.clone();
        let event_states = self.event_states.clone();
        let width = self.window.inner_size().width;
        let height = self.window.inner_size().height;
        let graphics = {
            let surface_locked = self.surface.clone();
            AAAGraphics::new(renderer, surface_locked, event_states, width, height)
        };
        self.graphics = Some(Arc::new(Mutex::new(graphics)));

        self.spawn_render_thread_and_render();
    }

    pub fn render_thread_close_join(&mut self) {
        self.event_states.exiting();
        if let Some(handle) = self.render_handle.take() {
            handle.join().unwrap();
        }
    }

    pub fn spawn_render_thread_and_render(&mut self) {
        self.event_states.opening();
        let graphics_locked = self.graphics.clone().unwrap();
        self.render_handle = Some(thread::spawn(move || {
            let mut graphics = graphics_locked.lock().unwrap();
            graphics.cycle();
        }));
    }
}

impl Drop for WindowState {
    fn drop(&mut self) {
        self.graphics = None;
        let surface_guard = self.surface.lock().unwrap();
        unsafe {
            // TODO move on its own struct
            self.renderer
                .surface_loader
                .destroy_surface(surface_guard.surface_khr, None)
        };
    }
}
