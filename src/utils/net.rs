// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::io::WithProgress;
use async_compression::tokio::bufread::ZstdDecoder;
use futures::TryStreamExt;
use std::io::ErrorKind;
use tap::Pipe;
use tokio::io::{AsyncBufReadExt, AsyncRead};
use tokio_util::either::Either::{Left, Right};
use tracing::info;
use url::Url;

use once_cell::sync::Lazy;

pub fn global_http_client() -> reqwest::Client {
    static CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);
    CLIENT.clone()
}

/// `location` may be:
/// - a path to a local file
/// - a URL to a web resource
/// - compressed
/// - uncompressed
///
/// This function returns a reader of uncompressed data.
pub async fn reader(location: &str) -> anyhow::Result<impl AsyncRead> {
    // This isn't the cleanest approach in terms of error-handling, but it works. If the URL is
    // malformed it'll end up trying to treat it as a local filepath. If that fails - an error
    // is thrown.
    let (stream, content_length) = match Url::parse(location) {
        Ok(url) => {
            info!("Downloading file: {}", url);
            let resp = reqwest::get(url).await?.error_for_status()?;
            let content_length = resp.content_length().unwrap_or_default();
            let stream = resp
                .bytes_stream()
                .map_err(|reqwest_error| std::io::Error::new(ErrorKind::Other, reqwest_error))
                .pipe(tokio_util::io::StreamReader::new);

            (Left(stream), content_length)
        }
        Err(_) => {
            info!("Reading file: {}", location);
            let stream = tokio::fs::File::open(location).await?;
            let content_length = stream.metadata().await?.len();
            (Right(stream), content_length)
        }
    };

    let mut reader = tokio::io::BufReader::new(WithProgress::wrap_async_read(
        "Loading",
        stream,
        content_length,
    ));

    Ok(match is_zstd(reader.fill_buf().await?) {
        true => Left(ZstdDecoder::new(reader)),
        false => Right(reader),
    })
}

// This method checks the header in order to see whether or not we are operating on a zstd
// archive. The zstd header has a maximum size of 18 bytes:
// https://github.com/facebook/zstd/blob/dev/doc/zstd_compression_format.md#zstandard-frames.
fn is_zstd(buf: &[u8]) -> bool {
    zstd_safe::get_frame_content_size(buf).is_ok()
}
