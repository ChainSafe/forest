// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! File download utilities with parallel connection support.
//!
//! This module provides high-performance file downloads similar to `aria2c -x5`,
//! using multiple parallel HTTP connections to download different parts of a file
//! simultaneously. A download is only split when the server honours range requests and every
//! connection would get at least [`MIN_PARALLEL_CHUNK_SIZE`]; anything smaller is fetched over a
//! single connection.
//!
//! # Configuration
//!
//! `FOREST_DOWNLOAD_CONNECTIONS` caps the number of parallel connections.
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
//! // Resumable downloads are split across connections when the file is large enough
//! download_to(&url, destination, DownloadFileOption::Resumable, None).await?;
//! # Ok(())
//! # }
//! ```

use crate::utils::encoding::hex;
use crate::utils::{RetryArgs, net::global_http_client, retry};
use anyhow::{Context as _, ensure};
use backon::{ExponentialBuilder, Retryable as _};
use base64::{Engine, prelude::BASE64_STANDARD};
use digest_io::IoWrapper;
use futures::stream::{self, StreamExt as _, TryStreamExt as _};
use human_repr::HumanCount as _;
use humantime::format_duration;
use md5::{Digest as _, Md5};
use std::sync::atomic::Ordering;
use std::{
    ffi::OsStr,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
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
        .max(1)
}

/// A chunk response that is not `206 Partial Content` means the server ignored our `Range`, which
/// retrying the identical request will not change.
#[derive(Debug, thiserror::Error)]
#[error("Chunk {chunk} was answered with {status} instead of 206 Partial Content")]
struct RangeIgnored {
    chunk: u64,
    status: http::StatusCode,
}

/// Chunks below this size cost more round trips than the extra connection saves, so a download is
/// split only once it is at least twice this size.
const MIN_PARALLEL_CHUNK_SIZE: u64 = 8 * 1024 * 1024;

fn num_chunks(total_size: u64, max_chunks: usize, min_chunk_size: u64) -> u64 {
    (max_chunks as u64).min(total_size / min_chunk_size.max(1))
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
    callback: Option<&(dyn Fn(String) + Sync + Send)>,
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
        Ok(file_md5) => match get_content_md5_hash_from_url(url.clone()).await? {
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
        Err(_) => false,
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

fn get_file_md5_hash(path: &Path) -> anyhow::Result<Vec<u8>> {
    let mut hasher = IoWrapper(Md5::new());
    let mut reader = BufReader::new(File::open(path)?);
    std::io::copy(&mut reader, &mut hasher)?;
    Ok(hasher.0.finalize().to_vec())
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
///
/// Returns `Ok(None)` without downloading anything when the file cannot be split: the server does
/// not honour range requests, or it is too small for `min_chunk_size`. The caller is then
/// responsible for downloading it over a single connection.
async fn download_http_parallel(
    url: &Url,
    directory: &Path,
    filename: &str,
    num_connections: usize,
    min_chunk_size: u64,
    callback: Option<Arc<dyn Fn(String) + Sync + Send>>,
) -> anyhow::Result<Option<PathBuf>> {
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

    if test_response.status() != http::StatusCode::PARTIAL_CONTENT {
        tracing::info!(
            %url,
            status = %test_response.status(),
            "Server doesn't support range requests"
        );
        return Ok(None);
    }

    // Parse Content-Range header: "bytes 0-0/12345" -> 12345
    let total_size = test_response
        .headers()
        .get(http::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split('/').nth(1))
        .and_then(|s| s.parse::<u64>().ok())
        .context("Failed to parse Content-Range header")?;

    drop(test_response);

    let chunks = num_chunks(total_size, num_connections, min_chunk_size);
    if chunks < 2 {
        tracing::debug!(%url, size = %total_size, "File too small to split across connections");
        return Ok(None);
    }

    // Create the file and allocate space
    let file = tokio::fs::File::create(&tmp_dst_path)
        .await
        .context("couldn't create destination file")?;
    file.set_len(total_size)
        .await
        .context("couldn't allocate file space")?;

    let chunk_size = total_size / chunks;

    tracing::debug!(
        %url,
        path = %dst_path.display(),
        size = %total_size,
        connections = %chunks,
        "downloading with parallel connections"
    );

    // Progress tracking - log every 5 seconds like the forest::progress system
    let bytes_downloaded = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let last_logged_bytes = Arc::new(std::sync::atomic::AtomicU64::new(0));
    // Store elapsed millis since start_time to avoid needing a Mutex<Instant>.
    let last_logged_millis = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let start_time = Instant::now();
    const UPDATE_FREQUENCY: Duration = Duration::from_secs(5);
    const UPDATE_FREQUENCY_MS: u64 = UPDATE_FREQUENCY.as_millis() as u64;

    // Download chunks in parallel
    let download_tasks = (0..chunks).map(|i| {
        let client = client.clone();
        let url = url.clone();
        let tmp_path = tmp_dst_path.clone();
        let bytes_downloaded = Arc::clone(&bytes_downloaded);
        let last_logged_bytes = Arc::clone(&last_logged_bytes);
        let last_logged_millis = Arc::clone(&last_logged_millis);
        let callback = callback.clone();

        let start = i * chunk_size;
        let end = if i == chunks - 1 {
            total_size - 1
        } else {
            ((i + 1) * chunk_size - 1).min(total_size - 1)
        };

        async move {
            let range = format!("bytes={start}-{end}");
            let expected_size = (end - start + 1) as usize;

            // Retry logic for each chunk
            let download_chunk = || async {
                let response = client
                    .get(url.clone())
                    .header(http::header::RANGE, &range)
                    .send()
                    .await?;

                let status = response.status();
                if status != http::StatusCode::PARTIAL_CONTENT {
                    return Err(RangeIgnored { chunk: i, status }.into());
                }

                // Open file for writing this chunk
                let mut file = tokio::fs::OpenOptions::new()
                    .write(true)
                    .open(&tmp_path)
                    .await?;
                file.seek(std::io::SeekFrom::Start(start)).await?;

                // Stream bytes and update progress incrementally
                let mut stream = response.bytes_stream();
                let mut chunk_bytes_written = 0u64;

                let result: anyhow::Result<()> = async {
                    while let Some(chunk_result) = stream.try_next().await? {
                        ensure!(
                            chunk_bytes_written + chunk_result.len() as u64 <= expected_size as u64,
                            "Chunk {i} overran its range of {expected_size} bytes"
                        );
                        file.write_all(&chunk_result).await?;
                        chunk_bytes_written += chunk_result.len() as u64;

                        let downloaded = bytes_downloaded
                            .fetch_add(chunk_result.len() as u64, Ordering::Relaxed)
                            + chunk_result.len() as u64;

                        // Log progress every 5 seconds (lockless fast path)
                        let elapsed_ms = start_time.elapsed().as_millis() as u64;
                        let prev_ms = last_logged_millis.load(Ordering::Relaxed);
                        if elapsed_ms.saturating_sub(prev_ms) >= UPDATE_FREQUENCY_MS
                            && last_logged_millis
                                // Spurious failure is fine — another task logs instead.
                                .compare_exchange_weak(
                                    prev_ms,
                                    elapsed_ms,
                                    Ordering::Relaxed,
                                    Ordering::Relaxed,
                                )
                                .is_ok()
                        {
                            let last_bytes = last_logged_bytes.load(Ordering::Relaxed);
                            let elapsed_secs = elapsed_ms as f64 / 1000.0;
                            let seconds_since_last = (elapsed_ms - prev_ms) as f64 / 1000.0;
                            let speed = downloaded.saturating_sub(last_bytes) as f64
                                / seconds_since_last.max(0.1);
                            let percent = downloaded
                                .checked_mul(100)
                                .and_then(|v| v.checked_div(total_size))
                                .unwrap_or(0);
                            tracing::info!(
                                target: "forest::progress",
                                "Loading {} / {}, {}%, {}/s, elapsed time: {}",
                                downloaded.human_count_bytes(),
                                total_size.human_count_bytes(),
                                percent,
                                speed.human_count_bytes(),
                                format_duration(Duration::from_secs(
                                    elapsed_secs as u64
                                ))
                            );

                            last_logged_bytes.store(downloaded, Ordering::Relaxed);
                        }

                        call_progress_callback(callback.as_deref(), downloaded, total_size);
                    }

                    file.flush().await?;
                    ensure!(
                        chunk_bytes_written == expected_size as u64,
                        "Chunk {i} size mismatch: expected {expected_size} \
                         bytes, got {chunk_bytes_written}"
                    );
                    Ok(())
                }
                .await;

                // On failure, undo progress so retries don't push past 100%.
                result.inspect_err(|e| {
                    tracing::warn!(
                        "Chunk {i} download failed after {}: {e:#}",
                        chunk_bytes_written.human_count_bytes(),
                    );
                    bytes_downloaded.fetch_sub(chunk_bytes_written, Ordering::Relaxed);
                })
            };

            download_chunk
                .retry(ExponentialBuilder::default().with_max_times(5))
                .when(|e: &anyhow::Error| !e.is::<RangeIgnored>())
                .await
                .with_context(|| format!("Failed to download chunk {i} after retries"))
        }
    });

    // Execute all downloads in parallel and collect results
    let results: Vec<_> = stream::iter(download_tasks)
        .buffer_unordered(chunks as usize)
        .collect()
        .await;

    // Check if any chunk failed
    for (i, result) in results.into_iter().enumerate() {
        result.with_context(|| format!("Chunk {i} failed"))?;
    }

    // Rename to final destination
    tokio::fs::rename(&tmp_dst_path, &dst_path)
        .await
        .context("couldn't rename file")?;

    tracing::debug!("successfully downloaded file to {}", dst_path.display());
    Ok(Some(dst_path))
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
    if let DownloadFileOption::Resumable = option {
        match download_http_parallel(
            url,
            directory,
            filename,
            get_num_download_connections(),
            MIN_PARALLEL_CHUNK_SIZE,
            callback.clone(),
        )
        .await
        {
            Ok(Some(path)) => return Ok(path),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("Parallel download failed ({e}), falling back to single connection")
            }
        }
    }

    download_http_single(url, directory, filename, option, callback).await
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
    use std::sync::LazyLock;
    use tokio::net::TcpListener;

    /// Test file data with known MD5 hash
    const TEST_FILE_CONTENT: &[u8] = b"ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn";

    /// MD5 hash of `TEST_FILE_CONTENT` (binary)
    fn test_file_md5() -> Vec<u8> {
        Md5::digest(TEST_FILE_CONTENT).to_vec()
    }

    /// Small enough to keep the test content chunkable, see [`num_chunks`].
    const TEST_MIN_CHUNK_SIZE: u64 = 16;

    /// Test server that supports range requests
    struct TestServer {
        addr: SocketAddr,
        shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
        /// `Range` header of every request served, `None` for an unranged one.
        requests: Arc<parking_lot::Mutex<Vec<Option<String>>>>,
    }

    impl TestServer {
        /// Start a new test server that serves `TEST_FILE_CONTENT` with range request support
        async fn start() -> Self {
            Self::start_with_content(TEST_FILE_CONTENT).await
        }

        /// Start a new test server with custom content
        async fn start_with_content(content: &'static [u8]) -> Self {
            let requests: Arc<parking_lot::Mutex<Vec<Option<String>>>> = Arc::default();
            let log = Arc::clone(&requests);
            let short_chunk_served = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let app = Router::new()
                .route(
                    "/test-file",
                    get(move |req: Request| {
                        let log = Arc::clone(&log);
                        async move {
                            log.lock().push(
                                req.headers()
                                    .get(header::RANGE)
                                    .and_then(|v| v.to_str().ok())
                                    .map(ToOwned::to_owned),
                            );
                            handle_file_request(req, content).await
                        }
                    }),
                )
                .route(
                    // Truncates the body of the first chunk request, then behaves.
                    "/test-file-short-first-chunk",
                    get(move |req: Request| {
                        let served = Arc::clone(&short_chunk_served);
                        async move {
                            let probe = req
                                .headers()
                                .get(header::RANGE)
                                .and_then(|v| v.to_str().ok())
                                == Some("bytes=0-0");
                            let mut response = handle_file_request(req, content).await;
                            if !probe && !served.swap(true, std::sync::atomic::Ordering::SeqCst) {
                                let body = std::mem::replace(response.body_mut(), Body::empty());
                                let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
                                let short = bytes.slice(..bytes.len().saturating_sub(1));
                                response.headers_mut().insert(
                                    header::CONTENT_LENGTH,
                                    short.len().to_string().parse().unwrap(),
                                );
                                *response.body_mut() = Body::from(short);
                            }
                            response
                        }
                    }),
                )
                .route(
                    // Honours the probe but ignores the range of every chunk request.
                    "/test-file-ignores-chunk-ranges",
                    get(move |req: Request| async move {
                        if req
                            .headers()
                            .get(header::RANGE)
                            .and_then(|v| v.to_str().ok())
                            == Some("bytes=0-0")
                        {
                            handle_file_request(req, content).await
                        } else {
                            Response::builder()
                                .status(StatusCode::OK)
                                .header(header::CONTENT_TYPE, "application/octet-stream")
                                .header(header::CONTENT_LENGTH, content.len())
                                .body(Body::from(content))
                                .unwrap()
                        }
                    }),
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
                    "/test-file-bad-content-range",
                    get(move |req: Request| async move {
                        if req.headers().contains_key(header::RANGE) {
                            let head = content.get(..1).unwrap_or_default();
                            Response::builder()
                                .status(StatusCode::PARTIAL_CONTENT)
                                .header(header::CONTENT_TYPE, "application/octet-stream")
                                .header(header::CONTENT_LENGTH, head.len())
                                .header(header::CONTENT_RANGE, "bytes totally-bogus")
                                .body(Body::from(head))
                                .unwrap()
                        } else {
                            handle_file_request(req, content).await
                        }
                    }),
                )
                .route(
                    "/test-file-with-md5-etag",
                    get(move |req: Request| async move {
                        let mut response = handle_file_request(req, content).await;
                        // Add MD5 hash as ETag (like filecoin-actors.chainsafe.dev)
                        let md5_hex = hex::encode(Md5::digest(content));
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
                        let md5 = Md5::digest(content);
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
                requests,
            }
        }

        fn url(&self, path: &str) -> Url {
            format!("http://{}{}", self.addr, path).parse().unwrap()
        }

        fn requests(&self) -> Vec<Option<String>> {
            self.requests.lock().clone()
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
                        .unwrap_or_else(|| content_len.saturating_sub(1));

                    // Handle empty content case
                    if content_len == 0 {
                        return Response::builder()
                            .status(StatusCode::RANGE_NOT_SATISFIABLE)
                            .header(header::CONTENT_RANGE, format!("bytes */{content_len}"))
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
                                    format!("bytes {start}-{end}/{content_len}"),
                                )
                                .header(header::ACCEPT_RANGES, "bytes")
                                .body(Body::from(range_content))
                                .unwrap();
                        } else {
                            // Range is out of bounds
                            return Response::builder()
                                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                                .header(header::CONTENT_RANGE, format!("bytes */{content_len}"))
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
    async fn test_download_http_small_file_uses_single_connection() {
        let server = TestServer::start().await;
        let temp_dir = tempfile::tempdir().unwrap();
        let url = server.url("/test-file");

        // Too small to be chunked, so this exercises the single-connection fallback of the dispatcher
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
        let downloaded_md5 = get_file_md5_hash(&result).unwrap();
        assert_eq!(downloaded_md5, test_file_md5());
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
            TEST_MIN_CHUNK_SIZE,
            Some(callback),
        )
        .await
        .unwrap()
        .unwrap();

        assert!(result.exists());

        // Progress is reported per body chunk and can move backwards when a chunk is retried,
        // so only the bound and the final value are guaranteed.
        let updates = progress_updates.lock();
        assert!(!updates.is_empty(), "Should have received progress updates");
        for update in updates.iter() {
            let percent: u8 = update.trim_end_matches('%').parse().unwrap();
            assert!(percent <= 100, "progress exceeded 100%: {update}");
        }
        assert_eq!(updates.last().map(String::as_str), Some("100%"));
    }

    #[tokio::test]
    async fn test_fallback_to_single_connection() {
        let server = TestServer::start().await;
        let temp_dir = tempfile::tempdir().unwrap();
        // Use the endpoint that doesn't support range requests
        let url = server.url("/test-file-no-ranges");

        assert!(
            download_http_parallel(&url, temp_dir.path(), "p.dat", 5, TEST_MIN_CHUNK_SIZE, None)
                .await
                .unwrap()
                .is_none()
        );

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

    #[test]
    fn test_num_chunks() {
        assert_eq!(num_chunks(0, 5, 8), 0);
        assert_eq!(num_chunks(7, 5, 8), 0);
        assert_eq!(num_chunks(8, 5, 8), 1);
        assert_eq!(num_chunks(16, 5, 8), 2);
        assert_eq!(num_chunks(u64::MAX, 5, 8), 5);
        assert_eq!(num_chunks(16, 0, 8), 0);
        assert_eq!(num_chunks(16, 5, 0), 5);
    }

    #[tokio::test]
    async fn test_download_http_falls_back_on_parallel_error() {
        let server = TestServer::start().await;
        let temp_dir = tempfile::tempdir().unwrap();
        let url = server.url("/test-file-bad-content-range");

        let err =
            download_http_parallel(&url, temp_dir.path(), "x.dat", 5, TEST_MIN_CHUNK_SIZE, None)
                .await
                .unwrap_err();
        assert!(format!("{err:#}").contains("Content-Range"), "{err:#}");

        let path = download_http(
            &url,
            temp_dir.path(),
            "fallback.dat",
            DownloadFileOption::Resumable,
            None,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), TEST_FILE_CONTENT);
    }

    fn generated_content(len: usize) -> &'static [u8] {
        static CONTENT: LazyLock<Vec<u8>> = LazyLock::new(|| {
            (0..2 * MIN_PARALLEL_CHUNK_SIZE as usize + 12345)
                .map(|i| (i % 251) as u8)
                .collect()
        });
        &CONTENT[..len]
    }

    async fn assert_download_http_yields(url: &Url, dir: &Path, name: &str, expected: &[u8]) {
        let path = download_http(url, dir, name, DownloadFileOption::Resumable, None)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), expected);
    }

    fn ranges_tile(requests: &[Option<String>], total: u64) -> bool {
        let mut spans: Vec<(u64, u64)> = requests
            .iter()
            .filter_map(|r| r.as_deref())
            .filter(|r| *r != "bytes=0-0")
            .map(|r| {
                let (start, end) = r.trim_start_matches("bytes=").split_once('-').unwrap();
                (start.parse().unwrap(), end.parse().unwrap())
            })
            .collect();
        spans.sort_unstable();
        spans.first().is_some_and(|(start, _)| *start == 0)
            && spans.last().is_some_and(|(_, end)| *end == total - 1)
            && spans
                .windows(2)
                .all(|w| w.first().map(|prev| prev.1 + 1) == w.get(1).map(|next| next.0))
    }

    #[tokio::test]
    async fn test_download_http_splits_only_above_threshold() {
        // Just over twice the minimum chunk size, so it must be split.
        let big = generated_content(2 * MIN_PARALLEL_CHUNK_SIZE as usize + 1);
        let server = TestServer::start_with_content(big).await;
        let dir = tempfile::tempdir().unwrap();
        assert_download_http_yields(&server.url("/test-file"), dir.path(), "big.dat", big).await;

        let requests = server.requests();
        assert_eq!(requests.first(), Some(&Some("bytes=0-0".to_owned())));
        assert!(
            requests.len() >= 3,
            "expected a probe and at least two chunks, got {requests:?}"
        );
        assert!(
            requests.iter().all(Option::is_some),
            "the whole file was fetched again over a single connection: {requests:?}"
        );
        assert!(
            ranges_tile(&requests, big.len() as u64),
            "chunk ranges must tile the file exactly: {requests:?}"
        );

        // Just over the minimum chunk size, which is still too small to split.
        let small = generated_content(MIN_PARALLEL_CHUNK_SIZE as usize + 1);
        let server = TestServer::start_with_content(small).await;
        assert_download_http_yields(&server.url("/test-file"), dir.path(), "small.dat", small)
            .await;

        assert_eq!(
            server.requests(),
            [Some("bytes=0-0".to_owned()), None],
            "expected a probe followed by one unranged GET"
        );
    }

    #[tokio::test]
    async fn test_short_chunk_body_is_retried_not_accepted() {
        let data = generated_content(2 * MIN_PARALLEL_CHUNK_SIZE as usize + 1);
        let server = TestServer::start_with_content(data).await;
        let dir = tempfile::tempdir().unwrap();
        let url = server.url("/test-file-short-first-chunk");

        let path = download_http_parallel(
            &url,
            dir.path(),
            "short.dat",
            5,
            MIN_PARALLEL_CHUNK_SIZE,
            None,
        )
        .await
        .unwrap()
        .expect("must use the parallel path");
        assert_eq!(std::fs::read(&path).unwrap(), data);
    }

    #[tokio::test]
    async fn test_chunk_answered_with_200_is_rejected_without_retrying() {
        let data = generated_content(2 * MIN_PARALLEL_CHUNK_SIZE as usize + 1);
        let server = TestServer::start_with_content(data).await;
        let dir = tempfile::tempdir().unwrap();
        let url = server.url("/test-file-ignores-chunk-ranges");

        let start = std::time::Instant::now();
        let err = download_http_parallel(
            &url,
            dir.path(),
            "ignored.dat",
            5,
            MIN_PARALLEL_CHUNK_SIZE,
            None,
        )
        .await
        .unwrap_err();

        assert!(format!("{err:#}").contains("instead of 206"), "{err:#}");
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "a server that ignores ranges must not be retried with backoff"
        );
        assert!(!dir.path().join("ignored.dat").exists());

        // The caller still gets the file over a single connection.
        assert_download_http_yields(&url, dir.path(), "ignored.dat", data).await;
    }

    #[tokio::test]
    async fn test_parallel_download_is_byte_exact_across_sizes() {
        for len in [0usize, 1, 15, 16, 17, 31, 32, 33, 65, 1023, 65537] {
            let data = generated_content(len);
            let server = TestServer::start_with_content(data).await;
            let dir = tempfile::tempdir().unwrap();
            let url = server.url("/test-file");

            for connections in [1usize, 3, 5] {
                let parallel = download_http_parallel(
                    &url,
                    dir.path(),
                    "m.dat",
                    connections,
                    TEST_MIN_CHUNK_SIZE,
                    None,
                )
                .await
                .unwrap();

                if num_chunks(len as u64, connections, TEST_MIN_CHUNK_SIZE) < 2 {
                    assert!(parallel.is_none(), "len={len} conns={connections}");
                } else {
                    let got = std::fs::read(parallel.unwrap()).unwrap();
                    assert_eq!(got, data, "content mismatch len={len} conns={connections}");
                }
            }

            assert_download_http_yields(&url, dir.path(), "h.dat", data).await;
        }
    }

    // Only this test pins `download_http` to the production threshold, so it pays for real bytes.
    #[tokio::test]
    async fn test_download_http_splits_at_production_threshold() {
        let data = generated_content(2 * MIN_PARALLEL_CHUNK_SIZE as usize + 12345);
        let server = TestServer::start_with_content(data).await;
        let dir = tempfile::tempdir().unwrap();
        let url = server.url("/test-file");

        assert!(
            download_http_parallel(
                &url,
                dir.path(),
                "big.dat",
                5,
                MIN_PARALLEL_CHUNK_SIZE,
                None
            )
            .await
            .unwrap()
            .is_some()
        );
        assert_download_http_yields(&url, dir.path(), "big2.dat", data).await;
    }
}
