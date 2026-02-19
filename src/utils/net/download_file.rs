// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! File download utilities with parallel connection support.
//!
//! This module provides high-performance file downloads similar to `aria2c -x5`,
//! using multiple parallel HTTP connections to download different parts of a file
//! simultaneously.
//!
//! # Configuration
//!
//! The number of parallel connections can be configured via the
//! `FOREST_DOWNLOAD_CONNECTIONS` environment variable:
//!
//! # Example
//!
//! ```no_run
//! use forest::doctest_private::{download_to, DownloadFileOption};
//! use url::Url;
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let url = Url::parse("https://example.com/large-file.zst")?;
//! let destination = Path::new("./large-file.zst");
//!
//! // Download with parallel connections (automatic for Resumable option)
//! download_to(&url, destination, DownloadFileOption::Resumable, None).await?;
//! # Ok(())
//! # }
//! ```

use crate::utils::{RetryArgs, net::global_http_client, retry};
use anyhow::{Context as _, ensure};
use backon::{ExponentialBuilder, Retryable as _};
use base64::{Engine, prelude::BASE64_STANDARD};
use futures::stream::{self, StreamExt as _, TryStreamExt as _};
use human_bytes::human_bytes;
use humantime::format_duration;
use md5::{Digest as _, Md5};
use std::sync::Arc;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use url::Url;

/// Number of parallel connections to use for downloads (like aria2c -x flag)
/// Can be overridden with `FOREST_DOWNLOAD_CONNECTIONS` environment variable
fn get_num_download_connections() -> usize {
    std::env::var("FOREST_DOWNLOAD_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5) // Default to 5 like aria2c -x5
}

/// Generate a temporary download path with `.frdownload` extension
fn gen_tmp_download_path(dst_path: &Path) -> PathBuf {
    const DOWNLOAD_EXTENSION: &str = "frdownload";
    let mut path = dst_path.to_path_buf();
    if let Some(ext) = path.extension() {
        path.set_extension(format!(
            "{}.{DOWNLOAD_EXTENSION}",
            ext.to_str().unwrap_or_default()
        ));
    } else {
        path.set_extension(DOWNLOAD_EXTENSION);
    }
    path
}

/// Call user-provided callback with progress percentage
fn call_progress_callback(
    callback: &Option<Arc<dyn Fn(String) + Sync + Send>>,
    downloaded: u64,
    total_size: u64,
) {
    if let Some(cb) = callback {
        let progress_pct = if total_size > 0 {
            ((downloaded as f64 / total_size as f64) * 100.0) as u8
        } else {
            0
        };
        cb(format!("{progress_pct}%"));
    }
}

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
    if let Some(cache_file_dir) = cache_file_path.parent()
        && !cache_file_dir.is_dir()
    {
        std::fs::create_dir_all(cache_file_dir)?;
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
            None,
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
        anyhow::bail!(
            "unsupported host, register in HOSTS_WITH_MD5_ETAG if it's known to use md5 as etag algorithm. url: {url}"
        )
    }
}

/// Download a file using multiple parallel connections (like aria2c -x5)
///
/// This function splits the file into chunks and downloads them in parallel,
/// which can significantly improve download speeds for large files.
async fn download_http_parallel(
    url: &Url,
    directory: &Path,
    filename: &str,
    num_connections: usize,
    callback: Option<Arc<dyn Fn(String) + Sync + Send>>,
) -> anyhow::Result<PathBuf> {
    ensure!(
        num_connections > 0,
        "Number of connections must be greater than 0"
    );
    if !directory.is_dir() {
        std::fs::create_dir_all(directory)?;
    }
    let dst_path = directory.join(filename);
    let tmp_dst_path = gen_tmp_download_path(&dst_path);

    let client = global_http_client();

    // Check if server supports range requests by attempting a small range request.
    // We test with an actual range request (bytes=0-0) instead of checking Accept-Ranges
    // header because:
    // 1. Some servers (especially CDNs with redirects) don't include Accept-Ranges in HEAD
    // 2. This follows redirects automatically and tests the final endpoint
    // 3. It's the same approach used by aria2c and other download managers
    // 4. Only costs 1 byte of bandwidth to verify
    let test_response = client
        .get(url.clone())
        .header(http::header::RANGE, "bytes=0-0")
        .send()
        .await?;

    // Server supports ranges if it returns 206 Partial Content
    let supports_ranges = test_response.status() == http::StatusCode::PARTIAL_CONTENT;

    // Get the actual file size from Content-Range or Content-Length
    let total_size = if supports_ranges {
        // Parse Content-Range header: "bytes 0-0/12345" -> 12345
        test_response
            .headers()
            .get(http::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split('/').nth(1))
            .and_then(|s| s.parse::<u64>().ok())
            .context("Failed to parse Content-Range header")?
    } else {
        // Fallback to Content-Length if range not supported
        test_response.content_length().unwrap_or(0)
    };

    if !supports_ranges || total_size == 0 {
        tracing::info!(
            %url,
            status = %test_response.status(),
            "Server doesn't support range requests, falling back to single connection"
        );
        return download_http_single(
            url,
            directory,
            filename,
            DownloadFileOption::Resumable,
            callback,
        )
        .await;
    }

    // Create the file and allocate space
    let file = tokio::fs::File::create(&tmp_dst_path)
        .await
        .context("couldn't create destination file")?;
    file.set_len(total_size)
        .await
        .context("couldn't allocate file space")?;

    // Prevent underflow when file is smaller than connection count
    // Use at most as many connections as there are bytes
    let effective_connections = (num_connections as u64).min(total_size.max(1));
    let chunk_size = total_size / effective_connections;

    tracing::debug!(
        %url,
        path = %dst_path.display(),
        size = %total_size,
        connections = %effective_connections,
        "downloading with parallel connections"
    );

    // Progress tracking - log every 5 seconds like the forest::progress system
    let bytes_downloaded = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let last_logged_bytes = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let last_logged_time = Arc::new(parking_lot::Mutex::new(Instant::now()));
    let start_time = Instant::now();
    const UPDATE_FREQUENCY: Duration = Duration::from_secs(5);

    // Download chunks in parallel
    let download_tasks = (0..effective_connections).map(|i| {
        let client = client.clone();
        let url = url.clone();
        let tmp_path = tmp_dst_path.clone();
        let bytes_downloaded = Arc::clone(&bytes_downloaded);
        let last_logged_bytes = Arc::clone(&last_logged_bytes);
        let last_logged_time = Arc::clone(&last_logged_time);
        let callback = callback.clone();

        let start = i * chunk_size;
        let end = if i == effective_connections - 1 {
            total_size - 1
        } else {
            ((i + 1) * chunk_size - 1).min(total_size - 1)
        };

        async move {
            let range = format!("bytes={}-{}", start, end);
            let expected_size = (end - start + 1) as usize;

            // Retry logic for each chunk
            let download_chunk = || async {
                let response = client
                    .get(url.clone())
                    .header(http::header::RANGE, &range)
                    .send()
                    .await?;

                if !response.status().is_success()
                    && response.status() != http::StatusCode::PARTIAL_CONTENT
                {
                    anyhow::bail!("Failed to download chunk {}: {}", i, response.status());
                }

                // Open file for writing this chunk
                let mut file = tokio::fs::OpenOptions::new()
                    .write(true)
                    .open(&tmp_path)
                    .await?;
                file.seek(std::io::SeekFrom::Start(start)).await?;

                // Stream bytes and update progress incrementally
                let mut stream = response.bytes_stream();
                let mut chunk_bytes_written = 0usize;

                while let Some(chunk_result) = stream.try_next().await? {
                    // Write this chunk of data
                    file.write_all(&chunk_result).await?;
                    chunk_bytes_written += chunk_result.len();

                    // Update global progress counter
                    let downloaded = bytes_downloaded.fetch_add(
                        chunk_result.len() as u64,
                        std::sync::atomic::Ordering::Relaxed,
                    ) + chunk_result.len() as u64;

                    // Log progress every 5 seconds (forest::progress format)
                    let now = Instant::now();
                    let mut last_logged = last_logged_time.lock();
                    if (now - *last_logged) > UPDATE_FREQUENCY {
                        let last_bytes =
                            last_logged_bytes.load(std::sync::atomic::Ordering::Relaxed);
                        let elapsed_secs = (now - start_time).as_secs_f64();
                        let seconds_since_last = (now - *last_logged).as_secs_f64().max(0.1);
                        let speed = (downloaded - last_bytes) as f64 / seconds_since_last;
                        let percent = if total_size > 0 {
                            downloaded * 100 / total_size
                        } else {
                            0
                        };

                        tracing::info!(
                            target: "forest::progress",
                            "Loading {} / {}, {}%, {}/s, elapsed time: {}",
                            human_bytes(downloaded as f64),
                            human_bytes(total_size as f64),
                            percent,
                            human_bytes(speed),
                            format_duration(Duration::from_secs(elapsed_secs as u64))
                        );

                        *last_logged = now;
                        last_logged_bytes.store(downloaded, std::sync::atomic::Ordering::Relaxed);
                    }

                    // Also call user callback if provided (for RPC state tracking)
                    call_progress_callback(&callback, downloaded, total_size);
                }

                file.flush().await?;

                // Verify we got the expected amount of data
                if chunk_bytes_written != expected_size {
                    anyhow::bail!(
                        "Chunk {} size mismatch: expected {} bytes, got {}",
                        i,
                        expected_size,
                        chunk_bytes_written
                    );
                }

                Ok::<_, anyhow::Error>(())
            };

            download_chunk
                .retry(ExponentialBuilder::default().with_max_times(5))
                .await
                .with_context(|| format!("Failed to download chunk {} after retries", i))
        }
    });

    // Execute all downloads in parallel and collect results
    let results: Vec<_> = stream::iter(download_tasks)
        .buffer_unordered(effective_connections as usize)
        .collect()
        .await;

    // Check if any chunk failed
    for (i, result) in results.into_iter().enumerate() {
        result.with_context(|| format!("Chunk {} failed", i))?;
    }

    // Rename to final destination
    tokio::fs::rename(&tmp_dst_path, &dst_path)
        .await
        .context("couldn't rename file")?;

    tracing::debug!("successfully downloaded file to {}", dst_path.display());
    Ok(dst_path)
}

/// Download the file at `url` with a single HTTP connection, returning the path to the downloaded file
async fn download_http_single(
    url: &Url,
    directory: &Path,
    filename: &str,
    option: DownloadFileOption,
    callback: Option<Arc<dyn Fn(String) + Sync + Send>>,
) -> anyhow::Result<PathBuf> {
    if !directory.is_dir() {
        std::fs::create_dir_all(directory)?;
    }
    let dst_path = directory.join(filename);
    let tmp_dst_path = gen_tmp_download_path(&dst_path);
    let destination = dst_path.display();
    tracing::info!(%url, %destination, "downloading with single connection");
    let mut reader = crate::utils::net::reader(url.as_str(), option, callback).await?;
    const WRITE_BUFFER_SIZE: usize = 1024 * 1024;
    let file = tokio::fs::File::create(&tmp_dst_path)
        .await
        .context("couldn't create destination file")?;
    let mut tempfile = tokio::io::BufWriter::with_capacity(WRITE_BUFFER_SIZE, file);
    tokio::io::copy(&mut reader, &mut tempfile)
        .await
        .context("couldn't download file")?;
    tempfile.flush().await.context("couldn't flush file")?;
    tokio::fs::rename(&tmp_dst_path, &dst_path)
        .await
        .context("couldn't rename file")?;
    Ok(dst_path)
}

/// Download the file at `url` using the global HTTP client (via [`download_http_parallel`] or
/// [`download_http_single`]), returning the path to the downloaded file.
///
/// Uses [`global_http_client`] for all HTTP requests.
pub async fn download_http(
    url: &Url,
    directory: &Path,
    filename: &str,
    option: DownloadFileOption,
    callback: Option<Arc<dyn Fn(String) + Sync + Send>>,
) -> anyhow::Result<PathBuf> {
    // Use parallel downloads for Resumable option, single connection otherwise
    match option {
        DownloadFileOption::Resumable => {
            let num_connections = get_num_download_connections();

            // Try parallel download, fall back to single connection on error
            match download_http_parallel(
                url,
                directory,
                filename,
                num_connections,
                callback.clone(),
            )
            .await
            {
                Ok(path) => Ok(path),
                Err(e) => {
                    tracing::warn!(
                        "Parallel download failed ({}), falling back to single connection",
                        e
                    );
                    download_http_single(
                        url,
                        directory,
                        filename,
                        DownloadFileOption::Resumable,
                        callback,
                    )
                    .await
                }
            }
        }
        DownloadFileOption::NonResumable => {
            download_http_single(url, directory, filename, option, callback).await
        }
    }
}

pub async fn download_file_with_retry(
    url: &Url,
    directory: &Path,
    filename: &str,
    option: DownloadFileOption,
    callback: Option<Arc<dyn Fn(String) + Sync + Send>>,
) -> anyhow::Result<PathBuf> {
    Ok(retry(
        RetryArgs {
            timeout: None,
            ..Default::default()
        },
        || download_http(url, directory, filename, option, callback.clone()),
    )
    .await?)
}

pub async fn download_to(
    url: &Url,
    destination: &Path,
    option: DownloadFileOption,
    callback: Option<Arc<dyn Fn(String) + Sync + Send>>,
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
        callback,
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use axum::{
        Router,
        body::Body,
        extract::Request,
        http::{StatusCode, header},
        response::Response,
        routing::get,
    };
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    /// Test file data with known MD5 hash
    const TEST_FILE_CONTENT: &[u8] = b"ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn";

    /// MD5 hash of `TEST_FILE_CONTENT` (binary)
    fn test_file_md5() -> Vec<u8> {
        let mut hasher = Md5::new();
        hasher.update(TEST_FILE_CONTENT);
        hasher.finalize().to_vec()
    }

    /// Test server that supports range requests
    struct TestServer {
        addr: SocketAddr,
        shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    }

    impl TestServer {
        /// Start a new test server that serves `TEST_FILE_CONTENT` with range request support
        async fn start() -> Self {
            Self::start_with_content(TEST_FILE_CONTENT).await
        }

        /// Start a new test server with custom content
        async fn start_with_content(content: &'static [u8]) -> Self {
            let app = Router::new()
                .route(
                    "/test-file",
                    get(move |req: Request| async move { handle_file_request(req, content).await }),
                )
                .route(
                    "/test-file-no-ranges",
                    get(move |_req: Request| async move {
                        // Server that doesn't support range requests
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, "application/octet-stream")
                            .header(header::CONTENT_LENGTH, content.len())
                            .body(Body::from(content))
                            .unwrap()
                    }),
                )
                .route(
                    "/test-file-with-md5-etag",
                    get(move |req: Request| async move {
                        let mut response = handle_file_request(req, content).await;
                        // Add MD5 hash as ETag (like filecoin-actors.chainsafe.dev)
                        let mut hasher = Md5::new();
                        hasher.update(content);
                        let md5_hex = hex::encode(hasher.finalize());
                        response
                            .headers_mut()
                            .insert(header::ETAG, format!("\"{md5_hex}\"").parse().unwrap());
                        response
                    }),
                )
                .route(
                    "/test-file-with-ms-blob-md5",
                    get(move |req: Request| async move {
                        let mut response = handle_file_request(req, content).await;
                        // Add MD5 hash as x-ms-blob-content-md5 (like GitHub releases)
                        let mut hasher = Md5::new();
                        hasher.update(content);
                        let md5 = hasher.finalize();
                        let md5_base64 = BASE64_STANDARD.encode(md5);
                        response
                            .headers_mut()
                            .insert("x-ms-blob-content-md5", md5_base64.parse().unwrap());
                        response
                    }),
                );

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();

            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

            tokio::spawn(async move {
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        shutdown_rx.await.ok();
                    })
                    .await
                    .unwrap();
            });

            Self {
                addr,
                shutdown_tx: Some(shutdown_tx),
            }
        }

        fn url(&self, path: &str) -> Url {
            format!("http://{}{}", self.addr, path).parse().unwrap()
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            // Trigger graceful shutdown (best effort, ignore errors)
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
        }
    }

    /// Handle file requests with range support
    async fn handle_file_request(req: Request, content: &'static [u8]) -> Response {
        let headers = req.headers();
        let content_len = content.len() as u64;

        // Check if this is a range request
        if let Some(range_header) = headers.get(header::RANGE)
            && let Ok(range_str) = range_header.to_str()
        {
            // Parse range header: "bytes=0-0" or "bytes=100-200"
            if let Some(range) = range_str.strip_prefix("bytes=") {
                let parts: Vec<&str> = range.split('-').collect();
                if parts.len() == 2 {
                    let start: u64 = parts
                        .first()
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);
                    let end: u64 = parts
                        .get(1)
                        .filter(|s| !s.is_empty())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(content_len.saturating_sub(1));

                    // Handle empty content case
                    if content_len == 0 {
                        return Response::builder()
                            .status(StatusCode::RANGE_NOT_SATISFIABLE)
                            .header(header::CONTENT_RANGE, format!("bytes */{}", content_len))
                            .body(Body::empty())
                            .unwrap();
                    }

                    let start = start.min(content_len - 1);
                    let end = end.min(content_len - 1);

                    if start <= end {
                        // Use .get() instead of direct indexing to safely handle edge cases
                        if let Some(range_content) = content.get(start as usize..=end as usize) {
                            return Response::builder()
                                .status(StatusCode::PARTIAL_CONTENT)
                                .header(header::CONTENT_TYPE, "application/octet-stream")
                                .header(header::CONTENT_LENGTH, range_content.len())
                                .header(
                                    header::CONTENT_RANGE,
                                    format!("bytes {}-{}/{}", start, end, content_len),
                                )
                                .header(header::ACCEPT_RANGES, "bytes")
                                .body(Body::from(range_content))
                                .unwrap();
                        } else {
                            // Range is out of bounds
                            return Response::builder()
                                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                                .header(header::CONTENT_RANGE, format!("bytes */{}", content_len))
                                .body(Body::empty())
                                .unwrap();
                        }
                    }
                }
            }
        }

        // Return full content
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CONTENT_LENGTH, content_len)
            .header(header::ACCEPT_RANGES, "bytes")
            .body(Body::from(content))
            .unwrap()
    }

    #[tokio::test]
    async fn test_get_content_md5_hash_from_url_1() {
        let server = TestServer::start().await;
        let url = server.url("/test-file-with-md5-etag");

        // This will fail because 127.0.0.1 is not in HOSTS_WITH_MD5_ETAG
        let md5 = get_content_md5_hash_from_url(url).await;
        assert!(
            md5.is_err(),
            "Should fail for localhost (not in HOSTS_WITH_MD5_ETAG)"
        );
    }

    #[tokio::test]
    async fn test_get_content_md5_hash_from_url_2() {
        let server = TestServer::start().await;
        let url = server.url("/test-file-with-ms-blob-md5");

        let md5 = get_content_md5_hash_from_url(url).await.unwrap();

        assert_eq!(md5, Some(test_file_md5()));
    }

    #[tokio::test]
    async fn test_download_file_with_cache() {
        let server = TestServer::start().await;
        let temp_dir = tempfile::tempdir().unwrap();
        let url = server.url("/test-file-with-ms-blob-md5");

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

    #[tokio::test]
    async fn test_parallel_download() {
        let server = TestServer::start().await;
        let temp_dir = tempfile::tempdir().unwrap();
        let url = server.url("/test-file");

        let result = download_http_parallel(
            &url,
            temp_dir.path(),
            "test-parallel.dat",
            3, // Use 3 connections for testing
            None,
        )
        .await
        .unwrap();

        assert!(result.exists());

        // Verify the file is not corrupted by checking its MD5
        let downloaded_md5 = get_file_md5_hash(&result);
        assert_eq!(downloaded_md5, Some(test_file_md5()));
    }

    #[tokio::test]
    async fn test_download_http_uses_parallel() {
        let server = TestServer::start().await;
        let temp_dir = tempfile::tempdir().unwrap();
        let url = server.url("/test-file");

        // Test with Resumable option (should use parallel)
        let result = download_http(
            &url,
            temp_dir.path(),
            "test-resumable.dat",
            DownloadFileOption::Resumable,
            None,
        )
        .await
        .unwrap();

        assert!(result.exists());

        // Verify integrity
        let downloaded_md5 = get_file_md5_hash(&result);
        assert_eq!(downloaded_md5, Some(test_file_md5()));
    }

    #[tokio::test]
    async fn test_parallel_download_with_progress() {
        let server = TestServer::start().await;
        let temp_dir = tempfile::tempdir().unwrap();
        let url = server.url("/test-file");

        // Track progress updates
        let progress_updates = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let progress_updates_clone = Arc::clone(&progress_updates);

        let callback = Arc::new(move |msg: String| {
            progress_updates_clone.lock().push(msg);
        });

        let result = download_http_parallel(
            &url,
            temp_dir.path(),
            "test-progress.dat",
            3,
            Some(callback),
        )
        .await
        .unwrap();

        assert!(result.exists());

        // Verify we got progress updates
        let updates = progress_updates.lock();
        assert!(!updates.is_empty(), "Should have received progress updates");

        // Verify progress increases monotonically
        let mut last_progress = 0;
        for update in updates.iter() {
            if let Some(progress_str) = update.strip_suffix('%')
                && let Ok(progress) = progress_str.parse::<u8>()
            {
                assert!(
                    progress >= last_progress,
                    "Progress should increase: {} < {}",
                    progress,
                    last_progress
                );
                last_progress = progress;
            }
        }

        // Should reach 100% for small test files
        assert!(
            last_progress >= 90,
            "Should reach at least 90% progress, got {}",
            last_progress
        );

        println!("Progress updates: {:?}", updates);
    }

    #[tokio::test]
    async fn test_fallback_to_single_connection() {
        let server = TestServer::start().await;
        let temp_dir = tempfile::tempdir().unwrap();
        // Use the endpoint that doesn't support range requests
        let url = server.url("/test-file-no-ranges");

        // Try to download with parallel (should fallback to single connection)
        let result = download_http(
            &url,
            temp_dir.path(),
            "test-fallback.dat",
            DownloadFileOption::Resumable,
            None,
        )
        .await
        .unwrap();

        assert!(result.exists());

        // Verify content is correct despite fallback
        let content = std::fs::read(&result).unwrap();
        assert_eq!(content, TEST_FILE_CONTENT);
    }

    #[tokio::test]
    async fn test_small_file_with_many_connections() {
        // Test edge case: file smaller than connection count
        // This tests the underflow prevention when chunk_size would be 0
        let small_content: &[u8] = b"Hi!"; // 3 bytes
        let server = TestServer::start_with_content(small_content).await;
        let temp_dir = tempfile::tempdir().unwrap();
        let url = server.url("/test-file");

        // Try to download with more connections than bytes
        let result = download_http_parallel(&url, temp_dir.path(), "tiny.dat", 5, None)
            .await
            .unwrap();

        assert!(result.exists());

        // Verify content is correct
        let downloaded = std::fs::read(&result).unwrap();
        assert_eq!(downloaded, small_content);
    }
}
