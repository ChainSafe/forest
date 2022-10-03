use std::pin::Pin;

use digest::{Digest, Output};
use futures::AsyncWrite;
use pin_project_lite::pin_project;

pin_project! {
    pub struct AsyncWriterWithChecksum<W,D> {
        pub inner: W,
        pub hasher: D

    }
}

impl<W: AsyncWrite + Unpin, D: Digest> AsyncWrite for AsyncWriterWithChecksum<W, D> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.hasher.update(buf);
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

impl<W, D: Digest> AsyncWriterWithChecksum<W, D> {
    pub fn finalize(self) -> Output<D> {
        self.hasher.finalize()
    }
}
