// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::io::BufRead;
use futures::prelude::*;
use isahc::{Body, HttpClient};
use pbr::ProgressBar;
use pin_project_lite::pin_project;
use std::convert::TryFrom;
use std::fs::File;
use std::io::{self, BufReader, Read, Result as IOResult, Stdout, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
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

impl<R: BufRead + Unpin, W: Write> BufRead for FetchProgress<R, W> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IOResult<&'_ [u8]>> {
        let this = self.project();
        this.inner.poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        Pin::new(&mut self.inner).consume(amt)
    }
}

impl<R: Read, W: Write> Read for FetchProgress<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
        self.inner.read(buf).map(|n| {
            self.progress_bar.add(n as u64);
            n
        })
    }
}

impl TryFrom<Url> for FetchProgress<Body, Stdout> {
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

        let request = client.get(url.as_str())?;

        let pb = ProgressBar::new(total_size);

        Ok(FetchProgress {
            progress_bar: pb,
            inner: request.into_body(),
        })
    }
}

impl TryFrom<File> for FetchProgress<BufReader<File>, Stdout> {
    type Error = Box<dyn std::error::Error>;

    fn try_from(file: File) -> Result<Self, Self::Error> {
        let total_size = file.metadata()?.len();

        let pb = ProgressBar::new(total_size);

        Ok(FetchProgress {
            progress_bar: pb,
            inner: BufReader::new(file),
        })
    }
}
