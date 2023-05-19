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
use crate::{io::ProgressBar, misc::Either};

// https://github.com/facebook/zstd/blob/dev/doc/zstd_compression_format.md#zstandard-frames
const ZSTD_MAGIC_HEADER: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

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
    pub async fn fetch_from_url(url: &Url) -> anyhow::Result<Self> {
        let (inner, progress_bar) = fetch_stream_from_url(url).await?;
        Ok(FetchProgress {
            inner,
            progress_bar,
        })
    }
}

impl FetchProgress<ZstdDecoder<DownloadStream>> {
    pub async fn fetch_zstd_compressed_from_url(url: &Url) -> anyhow::Result<Self> {
        let (inner, progress_bar) = fetch_stream_from_url(url).await?;
        let inner = ZstdDecoder::new(inner);
        Ok(FetchProgress {
            inner,
            progress_bar,
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
        let mut header = [0; ZSTD_MAGIC_HEADER.len()];
        file.read_exact(&mut header).await?;
        file.seek(SeekFrom::Start(0)).await?;
        header == ZSTD_MAGIC_HEADER
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

pub async fn get_fetch_progress_from_url(
    url: &Url,
) -> anyhow::Result<Either<FetchProgress<DownloadStream>, FetchProgress<ZstdDecoder<DownloadStream>>>>
{
    let (mut stream, _) = fetch_stream_from_url(url).await?;
    let is_zstd_compressed = {
        let mut header = [0; ZSTD_MAGIC_HEADER.len()];
        stream.read_exact(&mut header).await?;
        header == ZSTD_MAGIC_HEADER
    };
    log::info!("Loading {url}, is_zstd_compressed: {is_zstd_compressed}");
    if is_zstd_compressed {
        Ok(Either::Right(
            FetchProgress::fetch_zstd_compressed_from_url(url).await?,
        ))
    } else {
        Ok(Either::Left(FetchProgress::fetch_from_url(url).await?))
    }
}

async fn fetch_stream_from_url(url: &Url) -> anyhow::Result<(DownloadStream, ProgressBar)> {
    let client = https_client();
    let url = {
        let head_response = client
            .request(hyper::Request::head(url.as_str()).body("".into())?)
            .await?;

        // Use the redirect if available.
        match head_response.headers().get("location") {
            Some(url) => url.to_str()?.try_into()?,
            None => url.clone(),
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
    pb.message("Downloading/Importing snapshot ");
    pb.set_units(crate::io::progress_bar::Units::Bytes);
    pb.set_max_refresh_rate(Some(Duration::from_millis(500)));

    let map_err: fn(hyper::Error) -> futures::io::Error =
        |e| futures::io::Error::new(futures::io::ErrorKind::Other, e);
    let stream = response.into_body().map_err(map_err).into_async_read();

    Ok((stream, pb))
}
