// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_compression::tokio::bufread::ZstdDecoder;
use futures::TryStreamExt;

use indicatif::ProgressStyle;
use log::info;
use std::io::ErrorKind;
use tap::Pipe;
use tokio::io::AsyncBufReadExt;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tokio_util::either::Either::{Left, Right};
use url::Url;

pub struct StreamedContentReader {}

impl StreamedContentReader {
    /// This method is parsing a file-path/URL passed to it and attempts to open a stream.
    /// Additionally, it detects whether or not the resulting stream is a zstd archive and treats
    /// it accordingly.
    pub async fn read(path: &str) -> anyhow::Result<Box<dyn futures::AsyncRead + Send + Unpin>> {
        let read_progress = indicatif::ProgressBar::new_spinner().with_style(Self::spinner_style());
        // This isn't the cleanest approach in terms of error-handling, but it works. If the URL is
        // malformed it'll end up trying to treat it as a local filepath. If that fails - an error
        // is thrown.
        let (stream, content_length) = match Url::parse(path) {
            Ok(url) => {
                info!("downloading file: {}", url);
                let resp = reqwest::get(url).await?.error_for_status()?;
                let content_length = resp.content_length().unwrap_or_default();
                let stream = resp
                    .bytes_stream()
                    .map_err(|reqwest_error| std::io::Error::new(ErrorKind::Other, reqwest_error))
                    .pipe(tokio_util::io::StreamReader::new);

                (Left(stream), content_length)
            }
            _ => {
                info!("reading file: {}", path);
                let stream = tokio::fs::File::open(path).await?;
                let content_length = stream.metadata().await?.len();
                (Right(stream), content_length)
            }
        };

        if content_length > 0 {
            read_progress.set_length(content_length);
            read_progress.set_style(Self::progress_style())
        }

        let mut reader = tokio::io::BufReader::new(read_progress.wrap_async_read(stream));

        Ok(Box::new(
            match Self::is_zstd(reader.fill_buf().await?) {
                true => Left(ZstdDecoder::new(reader)),
                false => Right(reader),
            }
            .compat(),
        ))
    }

    // This method checks the header in order to see whether or not we are operating on a zstd
    // archive. The zstd header has a maximum size of 18 bytes:
    // https://github.com/facebook/zstd/blob/dev/doc/zstd_compression_format.md#zstandard-frames.
    fn is_zstd(buf: &[u8]) -> bool {
        zstd_safe::get_frame_content_size(buf).is_ok()
    }

    fn progress_style() -> ProgressStyle {
        indicatif::ProgressStyle::with_template(
            "{msg:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes}",
        )
        .expect("invalid progress template")
        .progress_chars("=>-")
    }

    fn spinner_style() -> ProgressStyle {
        ProgressStyle::with_template("{spinner:.blue} {msg}")
            .unwrap()
            .tick_strings(&[
                "▹▹▹▹▹",
                "▸▹▹▹▹",
                "▹▸▹▹▹",
                "▹▹▸▹▹",
                "▹▹▹▸▹",
                "▹▹▹▹▸",
                "▪▪▪▪▪",
            ])
    }
}
