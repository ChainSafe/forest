// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use humantime::format_duration;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use pin_project_lite::pin_project;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::ReadBuf;

use log::info;

const UPDATE_FREQUENCY: Duration = Duration::from_millis(5000);

pub fn wrap_async_read<R: tokio::io::AsyncRead>(
    message: &str,
    read: R,
    total_items: u64,
) -> WithProgressStream<R> {
    WithProgressStream {
        stream: read,
        progress: WithProgress::new(message, total_items),
    }
}

pin_project! {
    /// Wraps an iterator to display its progress.
    pub struct WithProgressStream<S> {
        #[pin]
        stream: S,
        progress: WithProgress,
    }
}

impl<R: tokio::io::AsyncRead> tokio::io::AsyncRead for WithProgressStream<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let prev_len = buf.filled().len() as u64;
        let this = self.project();
        if let Poll::Ready(e) = this.stream.poll_read(cx, buf) {
            this.progress.inc(buf.filled().len() as u64 - prev_len);
            Poll::Ready(e)
        } else {
            Poll::Pending
        }
    }
}

#[derive(Debug, Clone)]
struct WithProgress {
    completed_items: u64,
    frequency: Duration,
    start: Instant,
    last_logged: Instant,
    message: String,
    total_items: u64,
}

impl WithProgress {
    fn new(message: &str, total_items: u64) -> Self {
        let now = Instant::now();
        Self {
            completed_items: 0,
            frequency: UPDATE_FREQUENCY,
            start: now,
            last_logged: now,
            message: message.into(),
            total_items,
        }
    }

    fn inc(&mut self, value: u64) {
        self.completed_items += value;

        self.emit_log_if_required();
    }

    fn set(&mut self, value: u64) {
        self.completed_items = value;

        self.emit_log_if_required();
    }

    fn set_total(&mut self, value: u64) {
        self.total_items = value;

        self.emit_log_if_required();
    }

    fn emit_log_if_required(&mut self) {
        let now = Instant::now();
        if (now - self.last_logged) > self.frequency {
            let elapsed_secs = (now - self.start).as_secs_f64();
            let elapsed_duration = format_duration(Duration::from_secs(elapsed_secs as u64));

            let throughput = self.completed_items as f64 / elapsed_secs;

            let eta_secs =
                (self.total_items.saturating_sub(self.completed_items)) as f64 / throughput;
            let eta_duration = format_duration(Duration::from_secs(eta_secs as u64));

            info!(
                target: "forest::progress",
                "{} {} (elapsed: {}, eta: {})",
                self.message, self.completed_items, elapsed_duration, eta_duration
            );
            self.last_logged = now;
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProgressLog {
    progress: Arc<Mutex<WithProgress>>,
}

impl ProgressLog {
    pub fn new(message: &str, total_items: u64) -> Self {
        ProgressLog {
            progress: Arc::new(Mutex::new(WithProgress::new(message, total_items))),
        }
    }

    #[allow(dead_code)]
    pub fn inc(&self, value: u64) {
        self.progress.lock().inc(value);
    }

    pub fn set(&self, value: u64) {
        self.progress.lock().set(value);
    }

    pub fn set_total(&self, value: u64) {
        self.progress.lock().set_total(value);
    }
}
