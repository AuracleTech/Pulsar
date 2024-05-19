use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;

pub struct InputState {
    mouse_buttons: [AtomicBool; 3], // Assuming 3 buttons: left, right, middle
    mouse_pos_x: AtomicU32,
    mouse_pos_y: AtomicU32,
    keyboard_keys: [AtomicBool; 256], // Assuming 256 possible key codes
    window_width: AtomicU32,
    window_height: AtomicU32,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            mouse_buttons: [
                AtomicBool::new(false),
                AtomicBool::new(false),
                AtomicBool::new(false),
            ],
            mouse_pos_x: AtomicU32::new(0),
            mouse_pos_y: AtomicU32::new(0),
            keyboard_keys: [0; 256].map(|_| AtomicBool::new(false)),
            window_width: AtomicU32::new(800), // Default window size
            window_height: AtomicU32::new(600),
        }
    }
}

type SharedInputState = Arc<InputState>;
