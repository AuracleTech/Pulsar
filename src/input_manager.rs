use std::sync::atomic::{AtomicBool, Ordering};

pub struct EventStates {
    // mouse_buttons: [AtomicBool; 3], // Assuming 3 buttons: left, right, middle
    // mouse_pos_x: AtomicU32,
    // mouse_pos_y: AtomicU32,
    // keyboard_keys: [AtomicBool; 256], // Assuming 256 possible key codes
    pub exiting: AtomicBool,
}

impl EventStates {
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
            exiting: AtomicBool::new(false),
        }
    }
}
