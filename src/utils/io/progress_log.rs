use humantime::format_duration;
use std::time::{Duration, Instant};

use tracing::info;

pub struct ProgressLogBuilder {
    frequency: Option<Duration>,
    message: Option<String>,
}

impl ProgressLogBuilder {
    pub fn with_frequency(mut self, freq: Duration) -> Self {
        self.frequency = Some(freq);
        self
    }

    pub fn with_message(mut self, message: &str) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn start(self, total_size: u64) -> ProgressLog {
        let now = Instant::now();
        ProgressLog {
            start: now,
            last_logged: now,
            frequency: self.frequency.unwrap_or_else(|| Duration::from_secs(5)),
            size: 0,
            total_size,
            message: self.message.unwrap_or_default(),
        }
    }
}

pub struct ProgressLog {
    frequency: Duration,
    message: String,
    start: Instant,
    last_logged: Instant,
    size: u64,
    total_size: u64,
}

impl ProgressLog {
    pub fn builder() -> ProgressLogBuilder {
        ProgressLogBuilder {
            frequency: None,
            message: None,
        }
    }

    pub fn set(&mut self, current_size: u64) {
        self.update(current_size);
    }

    fn update(&mut self, current_size: u64) {
        self.size = current_size;
        let now = Instant::now();
        if (now - self.last_logged) > self.frequency {
            let elapsed_secs = (now - self.start).as_secs_f64();
            let elapsed_duration = format_duration(Duration::from_secs(elapsed_secs as u64));

            let throughput = self.size as f64 / elapsed_secs;
            let eta_secs = (self.total_size - self.size) as f64 / throughput;
            let eta_duration = format_duration(Duration::from_secs(eta_secs as u64));

            info!(
                "{} {} (elapsed: {}, eta: {})",
                self.message, current_size, elapsed_duration, eta_duration
            );
            self.last_logged = now;
        }
    }
}
