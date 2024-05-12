use std::time::{Duration, Instant};

const CYCLE_REPORT_INTERVAL: Duration = Duration::from_millis(1000);

#[derive(Debug)]
pub struct Metrics {
    pub start: Instant,
    pub cycle_start: Instant,
    pub frame_start: Instant,
    pub frame_end: Instant,
    pub slowest_render: Duration,
    pub fastest_render: Duration,
    pub total_render: Duration,
    pub total_frames: u32,
    pub delta_end_to_start: Duration,
    pub delta_start_to_start: Duration,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            cycle_start: Instant::now(),
            frame_start: Instant::now(),
            frame_end: Instant::now(),
            slowest_render: Duration::from_secs(0),
            fastest_render: Duration::from_secs(30),
            total_render: Duration::from_secs(0),
            total_frames: 0,
            delta_end_to_start: Duration::from_secs(0),
            delta_start_to_start: Duration::from_secs(0),
        }
    }
}

impl Metrics {
    pub fn start_frame(&mut self) {
        self.delta_end_to_start = self.frame_end.elapsed();
        self.delta_start_to_start = self.frame_start.elapsed();
        self.frame_start = Instant::now();
    }

    pub fn end_frame(&mut self) {
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
                "ΔEndStart {:?} Max(RenderTime) {:?} Min(RenderTime) {:?} x̄ {:?} t {} / {:?}s",
                self.delta_end_to_start,
                self.slowest_render,
                self.fastest_render,
                self.total_render / self.total_frames,
                self.total_frames,
                CYCLE_REPORT_INTERVAL.as_secs_f64()
            );
            *self = Self::default();
        }

        self.frame_end = Instant::now();
    }
}

#[macro_export]
macro_rules! stopwatch {
    ($func:expr) => {{
        use std::time::{Duration, Instant};

        let start_time = Instant::now();
        let result = $func;
        let elapsed = start_time.elapsed();
        log::debug!("{:?} tps ~{:.2}", elapsed, 1.0 / elapsed.as_secs_f64());

        result
    }};
}
