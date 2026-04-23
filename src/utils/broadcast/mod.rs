// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests;

use std::pin::Pin;

use futures::{FutureExt as _, Stream, StreamExt as _};
use tokio::sync::broadcast::{Receiver, Sender};
use tokio_stream::wrappers::BroadcastStream;

/// Returns `true` if there are any active subscribers to the given broadcast channel.
pub fn has_subscribers<T>(tx: &Sender<T>) -> bool {
    tx.closed().now_or_never().is_none()
}

/// Wraps a broadcast [`Receiver`] as a pinned [`Stream`] that skips `Lagged`
/// events and terminates on `Closed`.
///
/// Use this in place of a manual `rx.recv()` loop so the lagged/closed handling
/// stays DRY while each caller retains ownership of its own state across
/// iterations (no per-event `Arc::clone`s).
pub fn subscription_stream<T: Clone + Send + 'static>(
    rx: Receiver<T>,
) -> Pin<Box<dyn Stream<Item = T> + Send>> {
    Box::pin(BroadcastStream::new(rx).filter_map(|r| async move { r.ok() }))
}
