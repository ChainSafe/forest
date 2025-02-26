// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! It can often take time to perform some operations in Forest and we would like to have a way for logging progress.
//!
//! Previously we used progress bars thanks to the [`indicatif`](https://crates.io/crates/indicatif) library but we had a few issues with them:
//! - They behaved poorly together with regular logging
//! - They were too verbose and printed even for very small tasks (less than 5 seconds)
//! - They were only used when connected to a TTY and not written in log files
//!
//! This lead us to develop our own logging code.
//! This module provides two new types for logging progress that are [`WithProgress`] and [`WithProgressRaw`].
//! The main goal of [`WithProgressRaw`] is to maintain a similar API to the previous one from progress bar so we could remove the [`indicatif`](https://crates.io/crates/indicatif) dependency,
//! but, gradually, we would like to move to something better and use the [`WithProgress`] type.
//! The [`WithProgress`] type will provide a way to wrap user code while handling logging presentation details.
//! [`WithProgress`] is a wrapper that should extend to Iterators, Streams, Read/Write types. Right now it only wraps async reads.
//!
//! # Example
//! ```
//! use tokio_test::block_on;
//! use tokio::io::AsyncBufReadExt;
//! use forest::doctest_private::WithProgress;
//! block_on(async {
//!     let data: String = "some very big string".into();
//!     let mut reader = tokio::io::BufReader::new(data.as_bytes());
//!     let len = 0; // Compute total read length or find of way to estimate it
//!     // We just need to wrap our reader and use the wrapped version
//!     let reader_wp = tokio::io::BufReader::new(WithProgress::wrap_async_read("reading", reader, len));
//!     let mut stream = reader_wp.lines();
//!     while let Some(line) = stream.next_line().await.unwrap() {
//!         // Do something with the line
//!     }
//! })
//! ```
//! # Future work
//! - Add and move progressively to new API (Iterator, Streams), and removed deprecated usage of [`WithProgressRaw`]
//! - Add a more accurate ETA etc

use human_bytes::human_bytes;
use humantime::format_duration;
use std::time::{Duration, Instant};

use pin_project_lite::pin_project;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::ReadBuf;

const UPDATE_FREQUENCY: Duration = Duration::from_millis(5000);

pin_project! {
    #[derive(Debug, Clone)]
    pub struct WithProgress<Inner> {
        #[pin]
        inner: Inner,
        progress: Progress,
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
        if let Poll::Ready(e) = this.inner.poll_read(cx, buf) {
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
            inner: read,
            progress: Progress::new(message).with_total(total_items),
        }
    }

    pub fn bytes(mut self) -> Self {
        self.progress.item_type = ItemType::Bytes;
        self
    }
}

#[derive(Debug, Clone)]
struct Progress {
    completed_items: u64,
    total_items: Option<u64>,
    last_logged_items: u64,
    start: Instant,
    last_logged: Instant,
    message: String,
    item_type: ItemType,
}

#[derive(Debug, Clone, Copy)]
enum ItemType {
    Bytes,
    Items,
}

impl Progress {
    fn new(message: &str) -> Self {
        let now = Instant::now();
        Self {
            completed_items: 0,
            last_logged_items: 0,
            total_items: None,
            start: now,
            last_logged: now,
            message: message.into(),
            item_type: ItemType::Items,
        }
    }

    fn with_total(mut self, total: u64) -> Self {
        self.total_items = Some(total);
        self
    }

    fn inc(&mut self, value: u64) {
        self.completed_items += value;

        self.emit_log_if_required();
    }

    #[cfg(test)]
    fn set(&mut self, value: u64) {
        self.completed_items = value;

        self.emit_log_if_required();
    }

    // Example output:
    //
    // Bytes, with total: 12.4 MiB / 1.2 GiB, 1%, 1.5 MiB/s, elapsed time: 8m 12s
    // Bytes, without total: 12.4 MiB, 1.5 MiB/s, elapsed time: 8m 12s
    // Items, with total: 12 / 1200, 1%, 1.5 items/s, elapsed time: 8m 12s
    // Items, without total: 12, 1.5 items/s, elapsed time: 8m 12s
    fn msg(&self, now: Instant) -> String {
        let message = &self.message;
        let elapsed_secs = (now - self.start).as_secs_f64();
        let elapsed_duration = format_duration(Duration::from_secs(elapsed_secs as u64));
        // limit minimum duration to 0.1s to avoid inifinities.
        let seconds_since_last_msg = (now - self.last_logged).as_secs_f64().max(0.1);

        let at = match self.item_type {
            ItemType::Bytes => human_bytes(self.completed_items as f64),
            ItemType::Items => self.completed_items.to_string(),
        };

        let total = if let Some(total) = self.total_items {
            let mut output = String::new();
            if total > 0 {
                output += " / ";
                output += &match self.item_type {
                    ItemType::Bytes => human_bytes(total as f64),
                    ItemType::Items => total.to_string(),
                };
                output += &format!(", {}%", self.completed_items * 100 / total);
            }
            output
        } else {
            String::new()
        };

        let diff = (self.completed_items - self.last_logged_items) as f64 / seconds_since_last_msg;
        let speed = match self.item_type {
            ItemType::Bytes => format!("{}/s", human_bytes(diff)),
            ItemType::Items => format!("{diff:.0} items/s"),
        };

        format!("{message} {at}{total}, {speed}, elapsed time: {elapsed_duration}")
    }

    fn emit_log_if_required(&mut self) {
        let now = Instant::now();
        if (now - self.last_logged) > UPDATE_FREQUENCY {
            tracing::info!(
                target: "forest::progress",
                "{}",
                self.msg(now)
            );
            self.last_logged = now;
            self.last_logged_items = self.completed_items;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_msg_bytes() {
        let mut progress = Progress::new("test");
        let now = progress.start;
        progress.item_type = ItemType::Bytes;
        progress.total_items = Some(1024 * 1024 * 1024);
        progress.set(1024 * 1024 * 1024);
        progress.last_logged_items = 1024 * 1024 * 1024 / 2;
        // Going from 0MiB to 512MiB in 1s should show 512MiB/S
        assert_eq!(
            progress.msg(now + Duration::from_secs(1)),
            "test 1 GiB / 1 GiB, 100%, 512 MiB/s, elapsed time: 1s"
        );

        progress.set(1024 * 1024 * 1024 / 2);
        progress.last_logged_items = 1024 * 1024 * 128;
        // Going from 128MiB to 512MiB in 125s should show (512MiB-128MiB)/125s = ~3.1 MiB/s
        assert_eq!(
            progress.msg(now + Duration::from_secs(125)),
            "test 512 MiB / 1 GiB, 50%, 3.1 MiB/s, elapsed time: 2m 5s"
        );

        progress.set(1024 * 1024 * 1024 / 10);
        progress.last_logged_items = 1024 * 1024;
        // Going from 1MiB to 102.4MiB in 10s should show (102.4MiB-1MiB)/10s = ~10.1 MiB/s
        assert_eq!(
            progress.msg(now + Duration::from_secs(10)),
            "test 102.4 MiB / 1 GiB, 9%, 10.1 MiB/s, elapsed time: 10s"
        );
    }

    #[test]
    fn test_progress_msg_items() {
        let mut progress = Progress::new("test");
        let now = progress.start;
        progress.item_type = ItemType::Items;
        progress.total_items = Some(1024);
        progress.set(1024);
        progress.last_logged_items = 1024 / 2;
        assert_eq!(
            progress.msg(now + Duration::from_secs(1)),
            "test 1024 / 1024, 100%, 512 items/s, elapsed time: 1s"
        );

        progress.set(1024 / 2);
        progress.last_logged_items = 1024 / 3;
        assert_eq!(
            progress.msg(now + Duration::from_secs(125)),
            "test 512 / 1024, 50%, 1 items/s, elapsed time: 2m 5s"
        );

        progress.set(1024 / 10);
        progress.last_logged_items = 0;
        assert_eq!(
            progress.msg(now + Duration::from_secs(10)),
            "test 102 / 1024, 9%, 10 items/s, elapsed time: 10s"
        );
    }
}
