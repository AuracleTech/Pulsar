use std::time::{Duration, Instant};

const CYCLE_REPORT_INTERVAL: Duration = Duration::from_secs(1);
#[derive(Debug)]
pub(crate) struct Metrics {
    engine_start: Instant,
    cycle_start: Instant,
    frame_start: Instant,
    slowest_render: Duration,
    fastest_render: Duration,
    total_render: Duration,
    total_frames: u32,
}
impl Default for Metrics {
    fn default() -> Self {
        Self {
            engine_start: Instant::now(),
            cycle_start: Instant::now(),
            frame_start: Instant::now(),
            slowest_render: Duration::from_secs(0),
            fastest_render: Duration::from_secs(30),
            total_render: Duration::from_secs(0),
            total_frames: 0,
        }
    }
}
impl Metrics {
    pub(crate) fn start_frame(&mut self) {
        self.frame_start = Instant::now();
    }

    pub(crate) fn end_frame(&mut self) {
        self.total_frames += 1;
        let elapsed_time = self.frame_start.elapsed();
        self.total_render += elapsed_time;

        if elapsed_time > self.slowest_render {
            self.slowest_render = elapsed_time;
        }
        if elapsed_time < self.fastest_render {
            self.fastest_render = elapsed_time;
        }

        if self.cycle_start.elapsed() > CYCLE_REPORT_INTERVAL {
            log::info!(
                "Slowest {:?} Fastest {:?} Average {:?} Draw calls {}",
                self.slowest_render,
                self.fastest_render,
                self.total_render / self.total_frames,
                self.total_frames
            );
            *self = Self::default();
        }
    }
}

#[macro_export]
macro_rules! metrics {
    ($func:expr) => {{
        use std::time::{Duration, Instant};

        let start_time = Instant::now();
        let result = $func;
        let elapsed = start_time.elapsed();
        debug!("{:?} tps ~{:.2}", elapsed, 1.0 / elapsed.as_secs_f64());

        result
    }};
}

// pub(crate) fn print_memory_usage() {
//     use memory_stats::memory_stats;
//     if let Some(usage) = memory_stats() {
//         log::info!("Virtual memory usage: {}", format_size(usage.virtual_mem));
//         log::info!("Physical memory usage: {}", format_size(usage.physical_mem));
//     }
// }

// fn format_size(mut size: usize) -> String {
//     let units = ["B", "KB", "MB", "GB", "TB", "PB"];
//     let mut index = 0;

//     while size >= 1024 && index < units.len() - 1 {
//         size /= 1024;
//         index += 1;
//     }

//     format!("{} {}", size, units[index])
// }
