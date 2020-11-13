// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use isahc::{Body, HttpClient};
use pbr::ProgressBar;
use std::io::Stdout;
use std::io::{Read, Result as IOResult};
use thiserror::Error;
use url::Url;

/// Contains progress bar and reader.
pub struct DownloadProgress<R> {
    inner: R,
    progress_bar: ProgressBar<Stdout>,
}

#[derive(Debug, Error)]
enum DownloadError {
    #[error("Cannot read a file header")]
    HeaderError,
}

impl<R: Read> Read for DownloadProgress<R> {
    fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
        self.inner.read(buf).map(|n| {
            self.progress_bar.add(n as u64);
            n
        })
    }
}

/// Builds Reader for a provided URL.
pub fn make_reader(raw_url: String) -> Result<DownloadProgress<Body>, Box<dyn std::error::Error>> {
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

    let request = client.get(url.as_str())?;

    let pb = ProgressBar::new(total_size);

    Ok(DownloadProgress {
        progress_bar: pb,
        inner: request.into_body(),
    })
}
