// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::{
    fs,
    io::{self, copy, Read as AsyncRead},
};
use futures::task::{Context, Poll};
use indicatif::{ProgressBar, ProgressStyle};
use isahc::HttpClient;
use log::info;
use std::{marker::Unpin, path::Path, pin::Pin};
use thiserror::Error;
use url::Url;

/// Contains progress bar and reader.
struct DownloadProgress<S>
where
    S: Unpin,
{
    inner: S,
    progress_bar: ProgressBar,
}

#[derive(Debug, Error)]
enum DownloadError {
    #[error("Cannot read a file header")]
    HeaderError,
    #[error("Filename encoding error")]
    EncodingError,
}

impl<S: AsyncRead + Unpin> AsyncRead for DownloadProgress<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, io::Error>> {
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(size)) => {
                if size == 0 {
                    self.progress_bar.finish();
                } else {
                    self.progress_bar.inc(size as u64);
                }
                Poll::Ready(Ok(size))
            }
            rest => rest,
        }
    }
}

/// Downloads the file and returns the path where it's saved.
pub async fn download_file(raw_url: String) -> Result<String, Box<dyn std::error::Error>> {
    let url = Url::parse(raw_url.as_str())?;

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

    info!("Downloading file...");
    let mut request = client.get(url.as_str())?;

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .progress_chars("#>-"));

    let file = Path::new(
        url.path_segments()
            .and_then(|segments| segments.last())
            .unwrap_or("tmp.bin"),
    );

    let mut source = DownloadProgress {
        progress_bar: pb,
        inner: request.body_mut(),
    };

    let mut dest = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .await?;

    let _ = copy(&mut source, &mut dest).await?;

    info!("File has been downloaded");
    match file.to_str() {
        Some(st) => Ok(st.to_string()),
        None => Err(Box::new(DownloadError::EncodingError)),
    }
}
