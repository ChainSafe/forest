// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::{net::global_http_client, retry, RetryArgs};
use anyhow::Context as _;
use backon::{ExponentialBuilder, Retryable as _};
use base64::{prelude::BASE64_STANDARD, Engine};
use md5::{Digest as _, Md5};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    time::Duration,
};
use url::Url;

#[derive(Debug, Copy, Clone)]
pub enum DownloadFileOption {
    NonResumable,
    Resumable,
}

#[derive(Debug, Clone)]
pub struct DownloadFileResult {
    pub path: PathBuf,
    #[allow(dead_code)]
    pub cache_hit: bool,
}

pub async fn download_file_with_cache(
    url: &Url,
    cache_dir: &Path,
    option: DownloadFileOption,
) -> anyhow::Result<DownloadFileResult> {
    let cache_file_path =
        cache_dir.join(url.path().strip_prefix('/').unwrap_or_else(|| url.path()));
    if let Some(cache_file_dir) = cache_file_path.parent() {
        if !cache_file_dir.is_dir() {
            std::fs::create_dir_all(cache_file_dir)?;
        }
    }

    let cache_hit = match get_file_md5_hash(&cache_file_path) {
        Some(file_md5) => match get_content_md5_hash_from_url(url.clone()).await? {
            Some(url_md5) => {
                if file_md5 == url_md5 {
                    true
                } else {
                    tracing::warn!(
                        "download again due to md5 hash mismatch, url: {url}, local cache: {}, remote: {}",
                        hex::encode(&file_md5),
                        hex::encode(&url_md5)
                    );
                    false
                }
            }
            None => {
                anyhow::bail!("failed to extract md5 content hash from remote url {url}");
            }
        },
        None => false,
    };

    if cache_hit {
        tracing::debug!(%url, "loaded from cache");
    } else {
        download_file_with_retry(
            url,
            cache_file_path.parent().unwrap_or_else(|| Path::new(".")),
            cache_file_path
                .file_name()
                .and_then(OsStr::to_str)
                .with_context(|| {
                    format!(
                        "Error getting the file name of {}",
                        cache_file_path.display()
                    )
                })?,
            option,
        )
        .await?;
    }

    Ok(DownloadFileResult {
        path: cache_file_path,
        cache_hit,
    })
}

fn get_file_md5_hash(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok().map(|bytes| {
        let mut hasher = Md5::new();
        hasher.update(bytes.as_slice());
        hasher.finalize().to_vec()
    })
}

async fn get_content_md5_hash_from_url(url: Url) -> anyhow::Result<Option<Vec<u8>>> {
    const TIMEOUT: Duration = Duration::from_secs(5);
    let response = (|| {
        global_http_client()
            .head(url.clone())
            .timeout(TIMEOUT)
            .send()
    })
    .retry(ExponentialBuilder::default())
    .await?;
    let headers = response.headers();
    // Github release assets
    if let Some(ms_blob_md5) = headers.get("x-ms-blob-content-md5") {
        return Ok(Some(BASE64_STANDARD.decode(ms_blob_md5)?));
    }

    static HOSTS_WITH_MD5_ETAG: [&str; 2] =
        ["filecoin-actors.chainsafe.dev", ".digitaloceanspaces.com"];
    if url
        .host_str()
        .map(|h| HOSTS_WITH_MD5_ETAG.iter().any(|h_part| h.contains(h_part)))
        .unwrap_or_default()
    {
        let md5 = headers
            .get("etag")
            .and_then(|v| v.to_str().ok().map(|v| hex::decode(v.replace('"', ""))))
            .transpose()?;
        Ok(md5)
    } else {
        anyhow::bail!("unsupported host, register in HOSTS_WITH_MD5_ETAG if it's known to use md5 as etag algorithm. url: {url}")
    }
}

/// Download the file at `url` with a private HTTP client, returning the path to the downloaded file
pub async fn download_http(
    url: &Url,
    directory: &Path,
    filename: &str,
    option: DownloadFileOption,
) -> anyhow::Result<PathBuf> {
    if !directory.is_dir() {
        std::fs::create_dir_all(directory)?;
    }
    let dst_path = directory.join(filename);
    let destination = dst_path.display();
    tracing::info!(%url, %destination, "downloading snapshot");
    let mut reader = crate::utils::net::reader(url.as_str(), option).await?;
    let tmp_dst_path = {
        // like `crdownload` for the chrome browser
        const DOWNLOAD_EXTENSION: &str = "frdownload";
        let mut path = dst_path.clone();
        if let Some(ext) = path.extension() {
            path.set_extension(format!(
                "{}.{DOWNLOAD_EXTENSION}",
                ext.to_str().unwrap_or_default()
            ));
        } else {
            path.set_extension(DOWNLOAD_EXTENSION);
        }
        path
    };
    let mut tempfile = tokio::fs::File::create(&tmp_dst_path)
        .await
        .context("couldn't create destination file")?;
    tokio::io::copy(&mut reader, &mut tempfile)
        .await
        .context("couldn't download file")?;
    std::fs::rename(&tmp_dst_path, &dst_path).context("couldn't rename file")?;

    Ok(dst_path)
}

pub async fn download_file_with_retry(
    url: &Url,
    directory: &Path,
    filename: &str,
    option: DownloadFileOption,
) -> anyhow::Result<PathBuf> {
    Ok(retry(
        RetryArgs {
            timeout: None,
            ..Default::default()
        },
        || download_http(url, directory, filename, option),
    )
    .await?)
}

pub async fn download_to(
    url: &Url,
    destination: &Path,
    option: DownloadFileOption,
) -> anyhow::Result<()> {
    download_file_with_retry(
        url,
        destination.parent().with_context(|| {
            format!(
                "Error getting the parent directory of {}",
                destination.display()
            )
        })?,
        destination
            .file_name()
            .and_then(OsStr::to_str)
            .with_context(|| format!("Error getting the file name of {}", destination.display()))?,
        option,
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_get_content_md5_hash_from_url_1() {
        let url = "https://filecoin-actors.chainsafe.dev/v15.0.0/builtin-actors-mainnet.car";
        let md5 = get_content_md5_hash_from_url(url.try_into().unwrap())
            .await
            .unwrap()
            .map(hex::encode);
        assert_eq!(md5, Some("676b41e3dd1dc94430084bde84849701".into()))
    }

    #[tokio::test]
    async fn test_get_content_md5_hash_from_url_2() {
        let url = "https://github.com/filecoin-project/builtin-actors/releases/download/v15.0.0/builtin-actors-mainnet.car";
        let md5 = get_content_md5_hash_from_url(url.try_into().unwrap())
            .await
            .unwrap()
            .map(hex::encode);
        assert_eq!(md5, Some("676b41e3dd1dc94430084bde84849701".into()))
    }

    #[tokio::test]
    async fn test_download_file_with_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let url = "https://forest-snapshots.fra1.cdn.digitaloceanspaces.com/genesis/butterflynet-bafy2bzacecm7xklkq3hkc2kgm5wnb5shlxmffino6lzhh7lte5acytb7sssr4.car.zst".try_into().unwrap();
        let result =
            download_file_with_cache(&url, temp_dir.path(), DownloadFileOption::NonResumable)
                .await
                .unwrap();
        assert!(!result.cache_hit);
        let result =
            download_file_with_cache(&url, temp_dir.path(), DownloadFileOption::NonResumable)
                .await
                .unwrap();
        assert!(result.cache_hit);
    }
}
