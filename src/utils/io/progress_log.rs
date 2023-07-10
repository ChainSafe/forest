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

pin_project! {
    /// Wraps an iterator to display its progress.
    pub struct WithProgress<S> {
        #[pin]
        stream: S,
        progress: WithProgressInner,
    }
}

impl<R: tokio::io::AsyncRead> tokio::io::AsyncRead for WithProgress<R> {
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

impl<S> WithProgress<S> {
    pub fn wrap_async_read(message: &str, read: S, total_items: u64) -> WithProgress<S> {
        WithProgress {
            stream: read,
            progress: WithProgressInner::new(message, total_items),
        }
    }
}

#[derive(Debug, Clone)]
struct WithProgressInner {
    completed_items: u64,
    total_items: u64,
    start: Instant,
    last_logged: Instant,
    message: String,
}

impl WithProgressInner {
    fn new(message: &str, total_items: u64) -> Self {
        let now = Instant::now();
        Self {
            completed_items: 0,
            total_items,
            start: now,
            last_logged: now,
            message: message.into(),
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
        if (now - self.last_logged) > UPDATE_FREQUENCY {
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
pub struct WithProgressRaw {
    progress: Arc<Mutex<WithProgressInner>>,
}

impl WithProgressRaw {
    pub fn new(message: &str, total_items: u64) -> Self {
        WithProgressRaw {
            progress: Arc::new(Mutex::new(WithProgressInner::new(message, total_items))),
        }
    }

    pub fn set(&self, value: u64) {
        self.progress.lock().set(value);
    }

    pub fn set_total(&self, value: u64) {
        self.progress.lock().set_total(value);
    }
}
