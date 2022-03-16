// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::fs::File;
use async_std::io::BufReader;
use async_std::task;
use futures::prelude::*;
use isahc::{AsyncBody, HttpClient};
use pbr::{ProgressBar, Units};
use pin_project_lite::pin_project;
use std::convert::TryFrom;
use std::io::{self, Stdout, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
enum DownloadError {
    #[error("Cannot read a file header")]
    HeaderError,
}

pin_project! {
    /// Holds a Reader, tracks read progress and draw a progress bar.
    pub struct FetchProgress<R, W: Write> {
        #[pin]
        pub inner: R,
        pub progress_bar: ProgressBar<W>,
    }
}

impl<R, W: Write> FetchProgress<R, W> {
    pub fn finish(&mut self) {
        self.progress_bar.finish();
    }
}

impl<R: AsyncRead + Unpin, W: Write> AsyncRead for FetchProgress<R, W> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, io::Error>> {
        let r = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(size)) = r {
            self.progress_bar.add(size as u64);
        }
        r
    }
}

impl TryFrom<Url> for FetchProgress<AsyncBody, Stdout> {
    type Error = Box<dyn std::error::Error>;

    fn try_from(url: Url) -> Result<Self, Self::Error> {
        let client = HttpClient::new()?;
        let total_size = {
            let resp = client.head(url.as_str())?;
            if resp.status().is_success() {
                resp.headers()
                    .get("content-length")
                    .and_then(|ct_len| ct_len.to_str().ok())
                    .and_then(|ct_len| ct_len.parse().ok())
                    .unwrap_or(0)
            } else {
                return Err(Box::new(DownloadError::HeaderError));
            }
        };

        let request = task::block_on(client.get_async(url.as_str()))?;

        let mut pb = ProgressBar::new(total_size);
        pb.message("Downloading/Importing snapshot ");
        pb.set_units(Units::Bytes);
        pb.set_max_refresh_rate(Some(Duration::from_millis(500)));

        Ok(FetchProgress {
            progress_bar: pb,
            inner: request.into_body(),
        })
    }
}

impl TryFrom<File> for FetchProgress<BufReader<File>, Stdout> {
    type Error = Box<dyn std::error::Error>;

    fn try_from(file: File) -> Result<Self, Self::Error> {
        let total_size = async_std::task::block_on(file.metadata())?.len();

        let mut pb = ProgressBar::new(total_size);
        pb.message("Importing snapshot ");
        pb.set_units(Units::Bytes);
        pb.set_max_refresh_rate(Some(Duration::from_millis(500)));

        Ok(FetchProgress {
            progress_bar: pb,
            inner: BufReader::new(file),
        })
    }
}
