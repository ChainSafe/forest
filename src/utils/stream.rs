// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use futures::{Stream, StreamExt};

/// Decouple stream generation and stream consumption into separate threads,
/// keeping not-yet-consumed elements in a bounded queue. This is similar to
/// [`stream::buffered`](https://docs.rs/futures/latest/futures/stream/trait.StreamExt.html#method.buffered)
/// and
/// [`sink::buffer`](https://docs.rs/futures/latest/futures/sink/trait.SinkExt.html#method.buffer).
/// The key difference is that [`par_buffer`] is parallel rather than concurrent
/// and will make use of multiple cores when both the stream and the stream
/// consumer are CPU-bound. Because a new thread is spawned, the stream has to
/// be [`Sync`], [`Send`] and `'static`.
pub fn par_buffer<V: Send + Sync + 'static>(
    cap: usize,
    stream: impl Stream<Item = V> + Send + Sync + 'static,
) -> impl Stream<Item = V> {
    let (send, recv) = flume::bounded(cap);
    tokio::task::spawn(stream.map(Ok).forward(send.into_sink()));
    recv.into_stream()
}
