// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{pin::Pin, task::Poll};

use async_trait::async_trait;
use digest::{Digest, Output};
use futures::{io::BufWriter, AsyncWrite};
use futures_util::AsyncWriteExt;
use pin_project_lite::pin_project;

pin_project! {
    /// Wrapper `AsyncWriter` implementation that calculates the optional checksum on the fly.
    /// Both `Writer` and `Digest` parameters are generic so one can use freely the relevant
    /// structures, e.g. `BufWriter` and `Sha256`.
    pub struct AsyncWriterWithChecksum<D, W> {
        #[pin]
        inner: BufWriter<W>,
        hasher:Option<D>,
    }
}

/// Trait marking the object that is collecting a kind of a checksum.
#[async_trait]
pub trait Checksum<D: Digest> {
    /// Return the checksum and resets the internal hasher.
    async fn finalize(&mut self) -> std::io::Result<Option<Output<D>>>;
}

impl<D: Digest, W: AsyncWriteExt> AsyncWrite for AsyncWriterWithChecksum<D, W> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let mut this = self.project();
        let w = this.inner.poll_write(cx, buf);

        if let Some(hasher) = &mut this.hasher {
            if let Poll::Ready(Ok(size)) = w {
                if size > 0 {
                    hasher.update(&buf[..size]);
                }
            }
        }
        w
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.project().inner.poll_close(cx)
    }
}

#[async_trait]
impl<D: Digest + Send, W: AsyncWriteExt + Send> Checksum<D> for AsyncWriterWithChecksum<D, W> {
    async fn finalize(&mut self) -> std::io::Result<Option<Output<D>>> {
        if let Some(hasher) = &mut self.hasher {
            let hasher = std::mem::replace(hasher, D::new());
            Ok(Some(hasher.finalize()))
        } else {
            Ok(None)
        }
    }
}

impl<D: Digest, W> AsyncWriterWithChecksum<D, W> {
    pub fn new(writer: BufWriter<W>, checksum_enabled: bool) -> Self {
        Self {
            inner: writer,
            hasher: if checksum_enabled {
                Some(Digest::new())
            } else {
                None
            },
        }
    }
}

/// A void writer that does nothing but implements [`AsyncWrite`]
#[derive(Debug, Clone, Default)]
pub struct VoidAsyncWriter;

impl AsyncWrite for VoidAsyncWriter {
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

#[cfg(test)]
mod test {
    use anyhow::ensure;
    use futures::{io::BufWriter, AsyncWriteExt};
    use rand::{rngs::OsRng, RngCore};
    use sha2::{Sha256, Sha512};

    use super::*;

    #[tokio::test]
    async fn file_writer_fs_buf_writer() -> anyhow::Result<()> {
        let temp_file_path = tempfile::Builder::new().tempfile()?;
        let temp_file = async_fs::File::create(temp_file_path.path()).await?;
        let mut temp_file_writer =
            AsyncWriterWithChecksum::<Sha256, _>::new(BufWriter::new(temp_file), true);
        for _ in 0..1024 {
            let mut bytes = [0; 1024];
            OsRng.fill_bytes(&mut bytes);
            temp_file_writer.write_all(&bytes).await?;
        }

        temp_file_writer.flush().await?;
        temp_file_writer.close().await?;

        let checksum = temp_file_writer.finalize().await?;

        let file_hash = {
            let mut hasher = Sha256::default();
            let bytes = std::fs::read(temp_file_path.path())?;
            hasher.update(&bytes);
            Some(hasher.finalize())
        };

        ensure!(checksum == file_hash);

        Ok(())
    }

    #[tokio::test]
    async fn given_buffered_writer_and_sha256_digest_should_return_correct_checksum(
    ) -> anyhow::Result<()> {
        let buffer = Vec::new();
        let writer = BufWriter::new(buffer);

        let mut writer = AsyncWriterWithChecksum::<Sha256, _>::new(writer, true);

        let data = ["cthulhu", "azathoth", "dagon"];

        // Repeat to make sure the inner hasher can be properly reset
        for _ in 0..2 {
            for old_god in &data {
                writer.write_all(old_god.as_bytes()).await?;
            }

            assert_eq!(
                "3386191dc5c285074c3827452f4e3b685e3253f5b9ca7c4c2bb3f44d1263aef1",
                format!("{:x}", writer.finalize().await?.unwrap())
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn digest_of_nothing() -> anyhow::Result<()> {
        let buffer = Vec::new();
        let writer = BufWriter::new(buffer);
        let mut writer = AsyncWriterWithChecksum::<Sha512, _>::new(writer, true);
        assert_eq!(
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
            format!("{:x}", writer.finalize().await?.unwrap())
        );
        Ok(())
    }

    #[tokio::test]
    async fn no_checksum_of_nothing() -> anyhow::Result<()> {
        let buffer = Vec::new();
        let writer = BufWriter::new(buffer);
        let mut writer = AsyncWriterWithChecksum::<Sha512, _>::new(writer, false);
        ensure!(writer.finalize().await?.is_none());
        Ok(())
    }
}
