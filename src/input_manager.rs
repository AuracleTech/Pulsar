use crate::app::WIN_START_INNER_SIZE;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use winit::dpi::PhysicalSize;

pub struct EventStates {
    // mouse_buttons: [AtomicBool; 3], // Assuming 3 buttons: left, right, middle
    // mouse_pos_x: AtomicU32,
    // mouse_pos_y: AtomicU32,
    // keyboard_keys: [AtomicBool; 256], // Assuming 256 possible key codes
    pub window_width: AtomicU32,
    pub window_height: AtomicU32,
    pub minimized: AtomicBool,
    pub exiting: AtomicBool,
}

impl EventStates {
    #[inline]
    pub fn resize(&self, size: PhysicalSize<u32>) {
        let width = size.width;
        let height = size.height;

        self.window_width.store(width, Ordering::Relaxed);
        self.window_height.store(height, Ordering::Relaxed);

        if width == 0 || height == 0 {
            self.minimized.store(true, Ordering::Relaxed);
        } else {
            self.minimized.store(false, Ordering::Relaxed);
        }
    }

    #[inline]
    pub fn close_requested(&self) {
        self.exiting.store(true, Ordering::Relaxed);
    }
}

impl Default for EventStates {
    fn default() -> Self {
        Self {
            // mouse_buttons: [
            //     AtomicBool::new(false),
            //     AtomicBool::new(false),
            //     AtomicBool::new(false),
            // ],
            // mouse_pos_x: AtomicU32::new(0),
            // mouse_pos_y: AtomicU32::new(0),
            // keyboard_keys: [0; 256].map(|_| AtomicBool::new(false)),
            window_width: AtomicU32::new(WIN_START_INNER_SIZE.width),
            window_height: AtomicU32::new(WIN_START_INNER_SIZE.height),
            minimized: AtomicBool::new(false),
            exiting: AtomicBool::new(false),
        }
    }
}
