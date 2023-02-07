// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{pin::Pin, task::Poll};

use digest::{Digest, Output};
use pin_project_lite::pin_project;
use tokio::io::AsyncWrite;

pin_project! {
    /// Wrapper `AsyncWriter` implementation that calculates the checksum on the fly.
    /// Both `Writer` and `Digest` parameters are generic so one can use freely the relevant
    /// structures, e.g. `BufWriter` and `Sha256`.
    pub struct AsyncWriterWithChecksum<D, W> {
        #[pin]
        inner: W,
        hasher: D,
    }
}

/// Trait marking the object that is collecting a kind of a checksum.
pub trait Checksum<D: Digest> {
    /// Return the checksum and resets the internal hasher.
    fn finalize(&mut self) -> Output<D>;
}

impl<D: Digest, W: AsyncWrite + Unpin> AsyncWrite for AsyncWriterWithChecksum<D, W> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let w = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(size)) = w {
            self.hasher.update(&buf[0..size]);
        }
        w
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl<D: Digest, W> Checksum<D> for AsyncWriterWithChecksum<D, W> {
    fn finalize(&mut self) -> Output<D> {
        let hasher = std::mem::replace(&mut self.hasher, D::new());
        hasher.finalize()
    }
}

impl<D: Digest, W> AsyncWriterWithChecksum<D, W> {
    pub fn new(writer: W) -> Self {
        Self {
            inner: writer,
            hasher: Digest::new(),
        }
    }
}

#[cfg(test)]
mod test {
    use sha2::{Sha256, Sha512};
    use tokio::io::{AsyncWriteExt, BufWriter};

    use super::*;

    #[tokio::test]
    async fn given_buffered_writer_and_sha256_digest_should_return_correct_checksum() {
        let buffer = Vec::new();
        let writer = BufWriter::new(buffer);

        let mut writer = AsyncWriterWithChecksum::<Sha256, _>::new(writer);

        for old_god in ["cthulhu", "azathoth", "dagon"] {
            writer.write_all(old_god.as_bytes()).await.unwrap();
        }

        assert_eq!(
            "3386191dc5c285074c3827452f4e3b685e3253f5b9ca7c4c2bb3f44d1263aef1",
            format!("{:x}", writer.finalize())
        );
    }

    #[tokio::test]
    async fn digest_of_nothing() {
        let buffer = Vec::new();
        let writer = BufWriter::new(buffer);

        let mut writer = AsyncWriterWithChecksum::<Sha512, _>::new(writer);

        assert_eq!(
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
            format!("{:x}", writer.finalize())
        );
    }
}
