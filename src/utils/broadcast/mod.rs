// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests;

use futures::{FutureExt as _, Stream, StreamExt as _};
use std::pin::Pin;
use tokio::sync::broadcast::Receiver;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

/// Returns `true` if there are any active subscribers to the given broadcast channel.
pub fn has_subscribers<T>(tx: &tokio::sync::broadcast::Sender<T>) -> bool {
    tx.closed().now_or_never().is_none()
}

/// Wraps a broadcast [`Receiver`] as a pinned [`Stream`] that skips `Lagged`
/// events (logging the skip count at warn level) and terminates on `Closed`.
pub fn subscription_stream<T: Clone + Send + 'static>(
    rx: Receiver<T>,
) -> Pin<Box<dyn Stream<Item = T> + Send>> {
    Box::pin(BroadcastStream::new(rx).filter_map(|r| async move {
        match r {
            Ok(v) => Some(v),
            Err(BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!("broadcast subscription lagged: dropped {n} events");
                None
            }
        }
    }))
}
