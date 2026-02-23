// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod download_file;
pub use download_file::*;

use crate::utils::io::WithProgress;
use crate::utils::reqwest_resume;
use cid::Cid;
use futures::{AsyncWriteExt, TryStreamExt};
use reqwest::Response;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use tap::Pipe;
use tokio::io::AsyncBufRead;
use tokio_util::{
    compat::TokioAsyncReadCompatExt,
    either::Either::{Left, Right},
};
use tracing::info;
use url::Url;

pub fn global_http_client() -> reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);
    CLIENT.clone()
}

/// Download a file via IPFS HTTP gateway in trustless mode.
/// See <https://github.com/ipfs/specs/blob/main/http-gateways/TRUSTLESS_GATEWAY.md>
pub async fn download_ipfs_file_trustlessly(
    cid: &Cid,
    gateway: &Url,
    destination: &Path,
) -> anyhow::Result<()> {
    let url = {
        let mut url = gateway.join(&cid.to_string())?;
        url.set_query(Some("format=car"));
        Ok::<_, anyhow::Error>(url)
    }?;

    let tmp =
        tempfile::NamedTempFile::new_in(destination.parent().unwrap_or_else(|| Path::new(".")))?
            .into_temp_path();
    {
        let mut reader = reader(url.as_str(), DownloadFileOption::Resumable, None)
            .await?
            .compat();
        let mut writer = futures::io::BufWriter::new(async_fs::File::create(&tmp).await?);
        rs_car_ipfs::single_file::read_single_file_seek(&mut reader, &mut writer, Some(cid))
            .await?;
        writer.flush().await?;
        writer.close().await?;
    }

    tmp.persist(destination)?;

    Ok(())
}

/// `location` may be:
/// - a path to a local file
/// - a URL to a web resource
/// - compressed
/// - uncompressed
///
/// This function returns a reader of uncompressed data.
pub async fn reader(
    location: &str,
    option: DownloadFileOption,
    callback: Option<Arc<dyn Fn(String) + Sync + Send>>,
) -> anyhow::Result<impl AsyncBufRead> {
    // This isn't the cleanest approach in terms of error-handling, but it works. If the URL is
    // malformed it'll end up trying to treat it as a local filepath. If that fails - an error
    // is thrown.
    let (stream, content_length) = match Url::parse(location) {
        Ok(url) => {
            info!("Downloading file: {}", url);
            match option {
                DownloadFileOption::Resumable => {
                    let resume_resp = reqwest_resume::get(url).await?;
                    let resp = resume_resp.response().error_for_status_ref()?;
                    let content_length = resp.content_length().unwrap_or_default();
                    let stream = resume_resp
                        .bytes_stream()
                        .map_err(std::io::Error::other)
                        .pipe(tokio_util::io::StreamReader::new);
                    (Left(Left(stream)), content_length)
                }
                DownloadFileOption::NonResumable => {
                    let resp = global_http_client().get(url).send().await?;
                    let content_length = resp.content_length().unwrap_or_default();
                    let stream = resp
                        .bytes_stream()
                        .map_err(std::io::Error::other)
                        .pipe(tokio_util::io::StreamReader::new);
                    (Left(Right(stream)), content_length)
                }
            }
        }
        Err(_) => {
            info!("Reading file: {}", location);
            let stream = tokio::fs::File::open(location).await?;
            let content_length = stream.metadata().await?.len();
            (Right(stream), content_length)
        }
    };

    // Use a larger buffer (512KB) for better throughput on large files
    const DOWNLOAD_BUFFER_SIZE: usize = 512 * 1024;
    Ok(tokio::io::BufReader::with_capacity(
        DOWNLOAD_BUFFER_SIZE,
        WithProgress::wrap_sync_read_with_callback("Loading", stream, content_length, callback)
            .bytes(),
    ))
}

pub async fn http_get(url: &Url) -> anyhow::Result<Response> {
    info!(%url, "GET");
    Ok(global_http_client()
        .get(url.clone())
        .send()
        .await?
        .error_for_status()?)
}
