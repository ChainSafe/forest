// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::{
    io::BufReader,
    stream::{IntoAsyncRead, MapErr},
    AsyncRead, TryStreamExt,
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

impl<R> FetchProgress<R> {
    pub fn finish(&mut self) {
        self.progress_bar.finish();
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

        Ok(FetchProgress {
            progress_bar: pb,
            inner: stream,
        })
    }
}

impl FetchProgress<BufReader<async_fs::File>> {
    pub async fn fetch_from_file(file: async_fs::File) -> anyhow::Result<Self> {
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
