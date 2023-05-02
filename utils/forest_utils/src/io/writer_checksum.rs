// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{marker::PhantomData, pin::Pin, task::Poll};

use async_trait::async_trait;
use digest::{Digest, Output};
use futures::{io::BufWriter, AsyncWrite, AsyncWriteExt};
use pin_project_lite::pin_project;

pin_project! {
    /// Wrapper `AsyncWriter` implementation that calculates the checksum on the fly.
    /// Both `Writer` and `Digest` parameters are generic so one can use freely the relevant
    /// structures, e.g. `BufWriter` and `Sha256`.
    pub struct AsyncWriterWithChecksum<D, W> {
        #[pin]
        inner: BufWriter<W>,
        hasher: D,
    }
}

/// Trait marking the object that is collecting a kind of a checksum.
#[async_trait]
pub trait Checksum<D: Digest> {
    /// Return the checksum and resets the internal hasher.
    async fn finalize(&mut self) -> std::io::Result<Output<D>>;
}

impl<D: Digest, W: AsyncWrite + Unpin> AsyncWrite for AsyncWriterWithChecksum<D, W> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let w = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(size)) = w {
            if size > 0 {
                self.hasher.update(&buf[..size]);
            }
        }
        w
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

#[async_trait]
impl<D: Digest + Send, W: AsyncWrite + Send + Unpin> Checksum<D> for AsyncWriterWithChecksum<D, W> {
    async fn finalize(&mut self) -> std::io::Result<Output<D>> {
        self.inner.flush().await?;
        let hasher = std::mem::replace(&mut self.hasher, D::new());
        Ok(hasher.finalize())
    }
}

impl<D: Digest, W> AsyncWriterWithChecksum<D, W> {
    pub fn new(writer: BufWriter<W>) -> Self {
        Self {
            inner: writer,
            hasher: Digest::new(),
        }
    }
}

/// A void writer that does nothing but implements [`AsyncWrite`] and
/// [`Checksum`]
#[derive(Debug, Clone, Default)]
pub struct VoidAsyncWriterWithNoChecksum<D> {
    _d: PhantomData<D>,
}

impl<D: Digest> AsyncWrite for VoidAsyncWriterWithNoChecksum<D> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
}
#[async_trait]
impl<D: Digest + Send> Checksum<D> for VoidAsyncWriterWithNoChecksum<D> {
    async fn finalize(&mut self) -> std::io::Result<Output<D>> {
        Ok(Default::default())
    }
}

#[cfg(test)]
mod test {
    use anyhow::ensure;
    use rand::{rngs::OsRng, RngCore};
    use sha2::{Sha256, Sha512};

    use super::*;

    #[tokio::test]
    async fn file_writer_fs_buf_writer() -> anyhow::Result<()> {
        let temp_file_path = tempfile::Builder::new().tempfile()?;
        let temp_file = async_fs::File::create(temp_file_path.path()).await?;
        let mut temp_file_writer =
            AsyncWriterWithChecksum::<Sha256, _>::new(BufWriter::new(temp_file));
        for _ in 0..(1024 * 256) {
            let mut bytes = [0; 1024];
            OsRng.fill_bytes(&mut bytes);
            temp_file_writer.write_all(&bytes).await?;
        }

        let checksum = temp_file_writer.finalize().await?;

        let file_hash = {
            let mut hasher = Sha256::default();
            let bytes = std::fs::read(temp_file_path.path())?;
            hasher.update(&bytes);
            hasher.finalize()
        };

        ensure!(checksum == file_hash);

        Ok(())
    }

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
            format!("{:x}", writer.finalize().await.unwrap())
        );
    }

    #[tokio::test]
    async fn digest_of_nothing() {
        let buffer = Vec::new();
        let writer = BufWriter::new(buffer);

        let mut writer = AsyncWriterWithChecksum::<Sha512, _>::new(writer);

        assert_eq!(
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
            format!("{:x}", writer.finalize().await.unwrap())
        );
    }
}
