use futures::{Stream, StreamExt};

pub fn par_buffer<V: Send + Sync + 'static>(cap: usize, stream: impl Stream<Item = V> + Send + Sync + 'static) -> impl Stream<Item = V> {
    let (send, recv) = flume::bounded(cap);
    tokio::task::spawn(stream.map(Ok).forward(send.into_sink()));
    recv.into_stream()
}
