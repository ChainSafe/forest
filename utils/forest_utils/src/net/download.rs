// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    io::SeekFrom,
    path::Path,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use async_compression::futures::bufread::ZstdDecoder;
use futures::{
    io::BufReader,
    stream::{IntoAsyncRead, MapErr},
    AsyncRead, AsyncReadExt, AsyncSeekExt, TryStreamExt,
};
use pin_project_lite::pin_project;
use thiserror::Error;
use url::Url;

use super::https_client;
use crate::io::ProgressBar;

#[derive(Debug, Error)]
enum DownloadError {
    #[error("Cannot read a file header")]
    HeaderError,
}

pin_project! {
    /// Holds a Reader, tracks read progress and draw a progress bar.
    pub struct FetchProgress<R> {
        #[pin]
        pub inner: R,
        pub progress_bar: ProgressBar,
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for FetchProgress<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let r = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(size)) = r {
            self.progress_bar.add(size as u64);
        }
        r
    }
}

type DownloadStream = IntoAsyncRead<MapErr<hyper::Body, fn(hyper::Error) -> futures::io::Error>>;

impl FetchProgress<DownloadStream> {
    pub async fn fetch_from_url(url: Url) -> anyhow::Result<FetchProgress<DownloadStream>> {
        let client = https_client();

        let url = {
            let head_response = client
                .request(hyper::Request::head(url.as_str()).body("".into())?)
                .await?;

            // Use the redirect if available.
            match head_response.headers().get("location") {
                Some(url) => url.to_str()?.try_into()?,
                None => url,
            }
        };

        let total_size = {
            let resp = client
                .request(hyper::Request::head(url.as_str()).body("".into())?)
                .await?;
            if resp.status().is_success() {
                resp.headers()
                    .get("content-length")
                    .and_then(|ct_len| ct_len.to_str().ok())
                    .and_then(|ct_len| ct_len.parse().ok())
                    .unwrap_or(0)
            } else {
                return Err(anyhow::anyhow!(DownloadError::HeaderError));
            }
        };

        let response = client.get(url.as_str().try_into()?).await?;

        let pb = ProgressBar::new(total_size);
        pb.message("Downloading/Importing");
        pb.set_units(crate::io::progress_bar::Units::Bytes);
        pb.set_max_refresh_rate(Some(Duration::from_millis(500)));

        let map_err: fn(hyper::Error) -> futures::io::Error =
            |e| futures::io::Error::new(futures::io::ErrorKind::Other, e);
        let stream = response.into_body().map_err(map_err).into_async_read();

        Ok(FetchProgress {
            progress_bar: pb,
            inner: stream,
        })
    }
}

impl FetchProgress<BufReader<async_fs::File>> {
    async fn fetch_from_file(file: async_fs::File) -> anyhow::Result<Self> {
        let total_size = file.metadata().await?.len();

        let pb = ProgressBar::new(total_size);
        pb.message("Importing snapshot ");
        pb.set_units(crate::io::progress_bar::Units::Bytes);
        pb.set_max_refresh_rate(Some(Duration::from_millis(500)));

        Ok(FetchProgress {
            progress_bar: pb,
            inner: BufReader::new(file),
        })
    }
}

impl FetchProgress<ZstdDecoder<BufReader<async_fs::File>>> {
    async fn fetch_from_zstd_compressed_file(file: async_fs::File) -> anyhow::Result<Self> {
        let total_size = file.metadata().await?.len();

        let pb = ProgressBar::new(total_size);
        pb.message("Importing snapshot ");
        pb.set_units(crate::io::progress_bar::Units::Bytes);
        pb.set_max_refresh_rate(Some(Duration::from_millis(500)));

        let inner = ZstdDecoder::new(BufReader::new(file));

        Ok(FetchProgress {
            progress_bar: pb,
            inner,
        })
    }
}

pub async fn get_fetch_progress_from_file(
    file_path: impl AsRef<Path>,
) -> anyhow::Result<
    Either<
        FetchProgress<BufReader<async_fs::File>>,
        FetchProgress<ZstdDecoder<BufReader<async_fs::File>>>,
    >,
> {
    let mut file = async_fs::File::open(file_path.as_ref()).await?;
    let is_zstd_compressed = {
        let mut header = [0; 4];
        file.read_exact(&mut header).await?;
        file.seek(SeekFrom::Start(0)).await?;
        // https://github.com/facebook/zstd/blob/dev/doc/zstd_compression_format.md#zstandard-frames
        header == [0x28, 0xb5, 0x2f, 0xfd]
    };
    log::info!(
        "Loading {}, is_zstd_compressed: {is_zstd_compressed}",
        file_path.as_ref().display()
    );
    if is_zstd_compressed {
        Ok(Either::Right(
            FetchProgress::fetch_from_zstd_compressed_file(file).await?,
        ))
    } else {
        Ok(Either::Left(FetchProgress::fetch_from_file(file).await?))
    }
}

pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    fn left_mut(&mut self) -> Option<&mut L> {
        match self {
            Self::Left(left) => Some(left),
            _ => None,
        }
    }

    fn right_mut(&mut self) -> Option<&mut R> {
        match self {
            Self::Right(right) => Some(right),
            _ => None,
        }
    }
}

impl<L: AsyncRead + Unpin, R: AsyncRead + Unpin> AsyncRead for Either<L, R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        if let Some(left) = self.left_mut() {
            Pin::new(left).poll_read(cx, buf)
        } else if let Some(right) = self.right_mut() {
            Pin::new(right).poll_read(cx, buf)
        } else {
            panic!("This branch should never be hit")
        }
    }
}
