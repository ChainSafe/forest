// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    io::SeekFrom,
    path::Path,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use crate::auth::Error;
use async_compression::futures::bufread::ZstdDecoder;
use futures::{
    io::BufReader,
    stream::{IntoAsyncRead, MapErr},
    AsyncBufRead, AsyncRead, AsyncReadExt, AsyncSeekExt, TryStreamExt,
};
use log::info;
use pin_project_lite::pin_project;
use thiserror::Error;
use url::Url;

use super::https_client;
use crate::utils::{io::ProgressBar, misc::Either};

#[derive(Debug, Error)]
enum DownloadError {
    #[error("Cannot read a file header")]
    HeaderError,
}

pin_project! {
    /// Holds a Reader, tracks read progress and draws a progress bar.
    pub struct FetchProgress {
        #[pin]
        pub inner: Either<DownloadStream, BufReader<async_fs::File>>,
        pub progress_bar: ProgressBar,
    }
}

impl AsyncRead for FetchProgress {
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

impl AsyncBufRead for FetchProgress {
    fn poll_fill_buf(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<&[u8]>> {
        Pin::new(&mut Pin::get_mut(self).inner).poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        Pin::new(&mut self.inner).consume(amt);
        self.progress_bar.add(amt as u64);
    }
}

/// FileReader facilitates file streaming, whether local or remote.
pub struct FileReader {}

impl FileReader {
    /// Returns [`FetchProgress`]
    ///
    /// Auto-detects whether or not the file has to be streamed from a remote location, and
    /// whether or not it's zstd compressed.
    ///
    /// # Arguments
    ///
    /// * `path` - Snapshot location, could be either remote or local
    pub async fn read(
        path: &str,
    ) -> anyhow::Result<Either<ZstdDecoder<FetchProgress>, FetchProgress>> {
        let is_remote_file: bool = path.starts_with("http://") || path.starts_with("https://");
        let (stream, progress_bar) = match is_remote_file {
            true => {
                info!("Downloading file...");
                let url = Url::parse(path)?;
                let (reader, pb) = Self::fetch_stream_from_url(&url).await?;
                (Either::Left(reader), pb)
            }
            _ => {
                info!("Reading file...");
                let mut file = async_fs::File::open(path).await?;
                let (reader, pb) = Self::fetch_from_file(file).await?;

                (Either::Right(reader), pb)
            }
        };

        let reader = FetchProgress {
            inner: stream,
            progress_bar,
        };

        let reader = if Self::is_zstd(path) {
            Either::Left(ZstdDecoder::new(reader))
        } else {
            Either::Right(reader)
        };

        Ok(reader)
    }

    // Checks whether or not a file is a zstd archive by it's extension.
    fn is_zstd(path: &str) -> bool {
        path.ends_with(".zst")
    }

    async fn fetch_from_file(
        file: async_fs::File,
    ) -> anyhow::Result<(BufReader<async_fs::File>, ProgressBar)> {
        let total_size = file.metadata().await?.len();

        let pb = ProgressBar::new(total_size);
        pb.message("Importing snapshot ");
        pb.set_units(crate::utils::io::progress_bar::Units::Bytes);
        pb.set_max_refresh_rate(Some(Duration::from_millis(500)));

        Ok((BufReader::new(file), pb))
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
        pb.set_units(crate::utils::io::progress_bar::Units::Bytes);
        pb.set_max_refresh_rate(Some(Duration::from_millis(500)));

        let map_err: fn(hyper::Error) -> futures::io::Error =
            |e| futures::io::Error::new(futures::io::ErrorKind::Other, e);
        let stream = response.into_body().map_err(map_err).into_async_read();

        Ok((stream, pb))
    }
}

type DownloadStream = IntoAsyncRead<MapErr<hyper::Body, fn(hyper::Error) -> futures::io::Error>>;
