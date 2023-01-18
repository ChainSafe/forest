// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::https_client;
use crate::io::ProgressBar;
use futures::stream::{IntoAsyncRead, MapErr};
use futures::TryStreamExt;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use thiserror::Error;
use tokio::fs::File;
use tokio::io::AsyncRead;
use tokio::io::{BufReader, ReadBuf};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt};
use url::Url;

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

impl<R> FetchProgress<R> {
    pub fn finish(&mut self) {
        self.progress_bar.finish();
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for FetchProgress<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let prev_len = buf.filled().len();
        let r = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = r {
            self.progress_bar
                .add((buf.filled().len() - prev_len) as u64);
        }
        r
    }
}

type DownloadStream =
    Compat<IntoAsyncRead<MapErr<hyper::Body, fn(hyper::Error) -> futures::io::Error>>>;

impl FetchProgress<DownloadStream> {
    pub async fn fetch_from_url(url: Url) -> anyhow::Result<FetchProgress<DownloadStream>> {
        let client = https_client();
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
        let stream = response
            .into_body()
            .map_err(map_err)
            .into_async_read()
            .compat();

        Ok(FetchProgress {
            progress_bar: pb,
            inner: stream,
        })
    }
}

impl FetchProgress<BufReader<File>> {
    pub async fn fetch_from_file(file: File) -> anyhow::Result<Self> {
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
