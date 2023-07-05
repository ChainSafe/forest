// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use humantime::format_duration;
use std::time::{Duration, Instant};

use log::info;

const UPDATE_FREQUENCY: Duration = Duration::from_millis(1000);

pub fn wrap_iter<Inner>(
    message: &str,
    into_iter: impl IntoIterator<IntoIter = Inner>,
) -> WithProgressIter<Inner> {
    let inner = into_iter.into_iter();
    WithProgressIter {
        inner,
        progress: WithProgress::new(message),
    }
}

#[derive(Debug, Clone)]
pub struct WithProgressIter<Inner> {
    inner: Inner,
    progress: WithProgress,
}

impl<Inner> Iterator for WithProgressIter<Inner>
where
    Inner: Iterator,
{
    type Item = Inner::Item;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next() {
            Some(item) => {
                self.progress.emit_log_if_required(self.inner.size_hint());
                self.progress.inc();
                Some(item)
            }
            None => {
                // TODO handle fusing
                println!("finished {} items", self.progress.completed_items);
                None
            }
        }
    }
}

pub fn wrap_stream<S: futures_core::Stream>(message: &str, stream: S) -> WithProgressStream<S> {
    WithProgressStream {
        stream,
        progress: WithProgress::new(message),
    }
}

/// Wraps an iterator to display its progress.
#[derive(Debug)]
pub struct WithProgressStream<S> {
    pub(crate) stream: S,
    progress: WithProgress,
}

impl<S: futures_core::Stream + Unpin> futures_core::Stream for WithProgressStream<S> {
    type Item = S::Item;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let item = std::pin::Pin::new(&mut this.stream).poll_next(cx);
        match &item {
            std::task::Poll::Ready(Some(_)) => {
                this.progress.emit_log_if_required(this.stream.size_hint());
                this.progress.inc();
            }
            std::task::Poll::Ready(None) => this.progress.finish(),
            std::task::Poll::Pending => {}
        }
        item
    }
}

#[derive(Debug, Clone)]
struct WithProgress {
    completed_items: u64,
    frequency: Duration,
    start: Instant,
    last_logged: Instant,
    message: String,
}

impl WithProgress {
    fn new(message: &str) -> Self {
        let now = Instant::now();
        Self {
            completed_items: 0,
            frequency: UPDATE_FREQUENCY,
            start: now,
            last_logged: now,
            message: message.into(),
        }
    }

    fn inc(&mut self) {
        self.completed_items += 1;
    }

    fn emit_log_if_required(&mut self, size_hint: (usize, Option<usize>)) {
        let now = Instant::now();
        if (now - self.last_logged) > self.frequency {
            let elapsed_secs = (now - self.start).as_secs_f64();
            let elapsed_duration = format_duration(Duration::from_secs(elapsed_secs as u64));

            let throughput = self.completed_items as f64 / elapsed_secs;

            let (lower_bound, upper_bound) = size_hint;
            let total_items =
                upper_bound.unwrap_or(lower_bound) as u64 + self.completed_items;
            let eta_secs = (total_items - self.completed_items) as f64 / throughput;
            let eta_duration = format_duration(Duration::from_secs(eta_secs as u64));

            info!(
                "-> {} {} (elapsed: {}, eta: {})",
                self.message, self.completed_items, elapsed_duration, eta_duration
            );
            self.last_logged = now;
        }
    }

    fn finish(&mut self) {}
}
