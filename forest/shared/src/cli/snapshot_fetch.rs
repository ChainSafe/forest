// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    time::Duration,
};

use anyhow::bail;
use chrono::DateTime;
use forest_utils::{
    io::{progress_bar::Units, ProgressBar, TempFile},
    net::{
        https_client,
        hyper::{self, client::connect::Connect, Body, Response},
    },
};
use hex::{FromHex, ToHex};
use log::info;
use regex::Regex;
use s3::Bucket;
use sha2::{Digest, Sha256};
use time::{format_description, format_description::well_known::Iso8601, Date};
use tokio::{
    fs::{create_dir_all, File},
    io::{AsyncWriteExt, BufWriter},
};
use url::Url;

use super::Config;
use crate::cli::to_size_string;

/// Snapshot fetch service provider
#[derive(Debug)]
pub enum SnapshotServer {
    Forest,
    Filecoin,
}

impl FromStr for SnapshotServer {
    type Err = anyhow::Error;

    fn from_str(provider: &str) -> Result<Self, Self::Err> {
        match provider.to_lowercase().as_str() {
            "forest" => Ok(SnapshotServer::Forest),
            "filecoin" => Ok(SnapshotServer::Filecoin),
            _ => bail!(
                "Failed to fetch snapshot from: {provider}, Must be one of `forest`|`filecoin`."
            ),
        }
    }
}

/// Snapshot attributes
pub struct SnapshotInfo {
    pub network: String,
    pub date: Date,
    pub height: i64,
    pub path: PathBuf,
}

/// Collection of snapshots
pub struct SnapshotStore {
    pub snapshots: Vec<SnapshotInfo>,
}

impl SnapshotStore {
    pub fn new(config: &Config, snapshot_dir: &PathBuf) -> SnapshotStore {
        let mut snapshots = Vec::new();
        let pattern = Regex::new(
            r"^([^_]+?)_snapshot_(?P<network>[^_]+?)_(?P<date>\d{4}-\d{2}-\d{2})_height_(?P<height>\d+).car(.tmp|.aria2)?$",
        ).unwrap();
        if let Ok(dir) = std::fs::read_dir(snapshot_dir) {
            dir.flatten()
                .map(|entry| entry.path())
                .filter(|p| is_car_or_tmp(p))
                .for_each(|path| {
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        if let Some(captures) = pattern.captures(filename) {
                            let network: String = captures.name("network").unwrap().as_str().into();
                            if network == config.chain.name {
                                let date = Date::parse(
                                    captures.name("date").unwrap().as_str(),
                                    &Iso8601::DEFAULT,
                                )
                                .unwrap();
                                let height = captures
                                    .name("height")
                                    .unwrap()
                                    .as_str()
                                    .parse::<i64>()
                                    .unwrap();
                                let snapshot = SnapshotInfo {
                                    network,
                                    date,
                                    height,
                                    path,
                                };
                                snapshots.push(snapshot);
                            }
                        }
                    }
                });
        }
        SnapshotStore { snapshots }
    }

    pub fn display(&self) {
        self.snapshots
            .iter()
            .for_each(|s| println!("{}", s.path.display()));
    }
}

pub fn is_car_or_tmp(path: &Path) -> bool {
    let ext = path.extension().unwrap_or_default();
    ext == "car" || ext == "tmp" || ext == "aria2"
}

/// Fetches snapshot from a trusted location and saves it to the given
/// directory. Chain is inferred from configuration.
pub async fn snapshot_fetch(
    snapshot_out_dir: &Path,
    config: &Config,
    provider: &Option<SnapshotServer>,
    use_aria2: bool,
) -> anyhow::Result<PathBuf> {
    let server = match provider {
        Some(s) => s,
        None => match config.chain.name.to_lowercase().as_str() {
            "mainnet" => &SnapshotServer::Filecoin,
            "calibnet" => &SnapshotServer::Forest,
            _ => anyhow::bail!("Fetch not supported for chain {}", config.chain.name),
        },
    };
    match server {
        SnapshotServer::Forest => snapshot_fetch_forest(snapshot_out_dir, config, use_aria2).await,
        SnapshotServer::Filecoin => {
            snapshot_fetch_filecoin(snapshot_out_dir, config, use_aria2).await
        }
    }
}

/// Checks whether `aria2c` is available in PATH
pub fn is_aria2_installed() -> bool {
    which::which("aria2c").is_ok()
}

/// Fetches snapshot for `calibnet` from a default, trusted location. On
/// success, the snapshot will be saved in the given directory. In case of
/// failure (e.g. connection interrupted) it will not be removed.
async fn snapshot_fetch_forest(
    snapshot_out_dir: &Path,
    config: &Config,
    use_aria2: bool,
) -> anyhow::Result<PathBuf> {
    let snapshot_fetch_config = match config.chain.name.to_lowercase().as_str() {
        "mainnet" => bail!(
            "Mainnet snapshot fetch service not provided by Forest yet. Suggestion: use `--provider=filecoin` to fetch from Filecoin server."
        ),
        "calibnet" => &config.snapshot_fetch.forest.calibnet,
        _ => bail!("Fetch not supported for chain {}", config.chain.name,),
    };
    let name = &snapshot_fetch_config.bucket_name;
    let region = &snapshot_fetch_config.region;
    let bucket = Bucket::new_public(name, region.parse()?)?;

    // Grab contents of the bucket
    let bucket_contents = bucket.list(snapshot_fetch_config.path.clone(), Some("/".to_string()))?;

    // Find the the last modified file that is not a directory or empty file
    let last_modified = bucket_contents
        .first()
        .ok_or_else(|| anyhow::anyhow!("Couldn't list bucket"))?
        .contents
        .iter()
        .filter(|obj| obj.size > 0 && obj.key.rsplit_once('.').unwrap_or_default().1 == "car")
        .max_by_key(|obj| DateTime::parse_from_rfc3339(&obj.last_modified).unwrap_or_default())
        .ok_or_else(|| anyhow::anyhow!("Couldn't retrieve bucket contents"))?
        .to_owned();

    // Grab the snapshot name and create requested directory tree.
    let filename = last_modified.key.rsplit_once('/').unwrap().1;
    let snapshot_path = snapshot_out_dir.join(filename);
    create_dir_all(snapshot_out_dir).await?;

    // Download the file
    // It'd be better to use the bucket directly with `get_object_stream`, but at
    // the time of writing this code the Stream API is a bit lacking, making
    // adding a progress bar a pain. https://github.com/durch/rust-s3/issues/275
    let client = https_client();
    let snapshot_spaces_url = &snapshot_fetch_config.snapshot_spaces_url;
    let path = &snapshot_fetch_config.path;
    let url = snapshot_spaces_url.join(path)?.join(filename)?;

    let snapshot_response = client.get(url.as_str().try_into()?).await?;
    if use_aria2 {
        download_snapshot_and_validate_checksum_with_aria2(client, url, &snapshot_path).await?
    } else {
        let total_size = last_modified.size;
        download_snapshot_and_validate_checksum(
            client,
            url,
            &snapshot_path,
            snapshot_response,
            total_size,
        )
        .await?;
    }

    Ok(snapshot_path)
}

/// Fetches snapshot for `mainnet` from a default, trusted location. On success,
/// the snapshot will be saved in the given directory. In case of failure (e.g.
/// checksum verification fiasco) it will not be removed.
async fn snapshot_fetch_filecoin(
    snapshot_out_dir: &Path,
    config: &Config,
    use_aria2: bool,
) -> anyhow::Result<PathBuf> {
    let service_url = match config.chain.name.to_lowercase().as_ref() {
        "mainnet" => config.snapshot_fetch.filecoin.mainnet.clone(),
        "calibnet" => config.snapshot_fetch.filecoin.calibnet.clone(),
        _ => bail!("Fetch not supported for chain {}", config.chain.name,),
    };
    let client = https_client();

    let snapshot_url = {
        let head_response = client
            .request(hyper::Request::head(service_url.as_str()).body("".into())?)
            .await?;

        // Use the redirect if available.
        match head_response.headers().get("location") {
            Some(url) => url.to_str()?.try_into()?,
            None => service_url,
        }
    };

    let snapshot_response = client.get(snapshot_url.as_str().try_into()?).await?;

    // Grab the snapshot file name
    let filename = filename_from_url(&snapshot_url)?;
    // Create requested directory tree to store the snapshot
    create_dir_all(snapshot_out_dir).await?;
    let snapshot_name = normalize_filecoin_snapshot_name(&config.chain.name, &filename)?;
    let snapshot_path = snapshot_out_dir.join(&snapshot_name);
    // Download the file
    if use_aria2 {
        download_snapshot_and_validate_checksum_with_aria2(client, snapshot_url, &snapshot_path)
            .await?
    } else {
        let total_size = snapshot_response
            .headers()
            .get("content-length")
            .and_then(|ct_len| ct_len.to_str().ok())
            .and_then(|ct_len| ct_len.parse::<u64>().ok())
            .ok_or_else(|| anyhow::anyhow!("Couldn't retrieve content length"))?;

        download_snapshot_and_validate_checksum(
            client,
            snapshot_url,
            &snapshot_path,
            snapshot_response,
            total_size,
        )
        .await?;
    }
    Ok(snapshot_path)
}

/// Downloads snapshot to a file with a progress bar. Returns the digest of the
/// downloaded file.
async fn download_snapshot_and_validate_checksum<C>(
    client: hyper::Client<C>,
    url: Url,
    snapshot_path: &Path,
    snapshot_response: Response<Body>,
    total_size: u64,
) -> anyhow::Result<()>
where
    C: Connect + Clone + Send + Sync + 'static,
{
    info!("Snapshot url: {url}");
    info!(
        "Snapshot will be downloaded to {} ({})",
        snapshot_path.display(),
        to_size_string(&total_size.into())?
    );

    let progress_bar = ProgressBar::new(total_size);
    progress_bar.message("Downloading snapshot ");
    progress_bar.set_max_refresh_rate(Some(Duration::from_millis(500)));
    progress_bar.set_units(Units::Bytes);

    let snapshot_file_tmp = TempFile::new(snapshot_path.with_extension("car.tmp"));
    let file = File::create(snapshot_file_tmp.path()).await?;
    let mut writer = BufWriter::new(file);
    let mut downloaded: u64 = 0;
    let mut stream = snapshot_response.into_body();

    let mut snapshot_hasher = Sha256::new();
    while let Some(item) = futures::StreamExt::next(&mut stream).await {
        let chunk = item?;
        writer.write_all(&chunk).await?;
        downloaded = total_size.min(downloaded + chunk.len() as u64);
        progress_bar.set(downloaded);
        snapshot_hasher.update(chunk);
    }
    writer.flush().await?;

    let file_size = std::fs::metadata(snapshot_file_tmp.path())?.len();
    if file_size != total_size {
        bail!("Didn't manage to download the entire file. {file_size}/{total_size} [B]");
    }

    progress_bar.finish_println("Finished downloading the snapshot.");

    fetch_checksum_and_validate(client, url, &snapshot_hasher.finalize()).await?;
    std::fs::rename(snapshot_file_tmp.path(), snapshot_path)?;

    Ok(())
}

async fn download_snapshot_and_validate_checksum_with_aria2<C>(
    client: hyper::Client<C>,
    url: Url,
    snapshot_path: &Path,
) -> anyhow::Result<()>
where
    C: Connect + Clone + Send + Sync + 'static,
{
    info!("Snapshot url: {url}");
    info!("Snapshot will be downloaded to {}", snapshot_path.display());

    if !is_aria2_installed() {
        bail!("Command aria2c is not in PATH. To install aria2, refer to instructions on https://aria2.github.io/");
    }

    let checksum_url = replace_extension_url(url.clone(), "sha256sum")?;
    let checksum_response = client.get(checksum_url.as_str().try_into()?).await?;
    if !checksum_response.status().is_success() {
        bail!("Unable to get the checksum file. Url: {checksum_url}");
    }
    let checksum_bytes = hyper::body::to_bytes(checksum_response.into_body()).await?
        [..Sha256::output_size() * 2]
        .to_vec();
    let checksum_expected = String::from_utf8(checksum_bytes)?;
    info!("Expected sha256 checksum: {checksum_expected}");
    download_with_aria2(
        url.as_str(),
        snapshot_path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or_default(),
        snapshot_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or_default(),
        format!("sha-256={checksum_expected}").as_str(),
    )
}

fn download_with_aria2(url: &str, dir: &str, out: &str, checksum: &str) -> anyhow::Result<()> {
    let mut child = Command::new("aria2c")
        .args([
            "--continue=true",
            "--max-connection-per-server=5",
            "--split=5",
            "--max-tries=0",
            &format!("--checksum={checksum}"),
            &format!("--dir={dir}",),
            &format!("--out={out}",),
            url,
        ])
        .spawn()?;

    let exit_code = child.wait()?;
    if exit_code.success() {
        Ok(())
    } else {
        // https://aria2.github.io/manual/en/html/aria2c.html#exit-status
        bail!(match exit_code.code() {
            Some(32) => "Checksum validation failed".into(),
            Some(code) =>format!("Failed with exit code {code}, checkout https://aria2.github.io/manual/en/html/aria2c.html#exit-status"),
            None => "Failed with unknown exit code.".into(),
        });
    }
}

/// Tries to extract resource filename from a given URL.
fn filename_from_url(url: &Url) -> anyhow::Result<String> {
    let filename = url
        .path_segments()
        .ok_or_else(|| anyhow::anyhow!("Can't parse url: {url}"))?
        .last()
        .unwrap() // safe, there is at least one
        .to_owned();

    if filename.is_empty() {
        Err(anyhow::anyhow!("can't extract filename from {url}"))
    } else {
        Ok(filename)
    }
}

/// Returns a normalized snapshot name
/// Filecoin snapshot files are named in the format of
/// `<height>_<YYYY_MM_DD>T<HH_MM_SS>Z.car`. Normalized snapshot name are in the
/// format `filecoin_snapshot_{mainnet|calibnet}_<YYYY-MM-DD>_height_<height>.
/// car`. # Example
/// ```
/// # use forest_cli_shared::cli::normalize_filecoin_snapshot_name;
/// let actual_name = "64050_2022_11_24T00_00_00Z.car";
/// let normalized_name = "filecoin_snapshot_calibnet_2022-11-24_height_64050.car";
/// assert_eq!(normalized_name, normalize_filecoin_snapshot_name("calibnet", actual_name).unwrap());
/// ```
pub fn normalize_filecoin_snapshot_name(network: &str, filename: &str) -> anyhow::Result<String> {
    let pattern = Regex::new(
        r"(?P<height>\d+)_(?P<date>\d{4}_\d{2}_\d{2})T(?P<time>\d{2}_\d{2}_\d{2})Z.car$",
    )
    .unwrap();
    if let Some(captures) = pattern.captures(filename) {
        let date = Date::parse(
            captures.name("date").unwrap().as_str(),
            &format_description::parse("[year]_[month]_[day]").unwrap(),
        )?;
        let height = captures.name("height").unwrap().as_str().parse::<i64>()?;
        Ok(format!(
            "filecoin_snapshot_{network}_{}_height_{height}.car",
            date.format(&format_description::parse("[year]-[month]-[day]").unwrap())?
        ))
    } else {
        bail!("Cannot parse filename: {filename}");
    }
}

/// Return a path with changed extension from a given URL.
fn replace_extension_url(mut url: Url, extension: &str) -> anyhow::Result<Url> {
    let new_filename = url
        .path_segments()
        .ok_or_else(|| anyhow::anyhow!("Can't parse url: {url} - no path segments"))?
        .last()
        .ok_or_else(|| anyhow::anyhow!("Can't parse url: {url} - can't get last path segment"))?
        .rsplit_once('.')
        .ok_or_else(|| anyhow::anyhow!("Can't parse url: {url} - no extension"))?
        .0
        .to_owned()
        + "."
        + extension;

    url.path_segments_mut()
        .iter_mut()
        .last()
        .unwrap() // safe - would've failed sooner
        .pop()
        .push(&new_filename);

    Ok(url)
}

/// Fetches the relevant checksum for the snapshot, compares it with the result
/// one. Fails if they don't match. The checksum is expected to be located in
/// the same location as the snapshot but with a `.sha256sum` extension.
async fn fetch_checksum_and_validate<C>(
    client: hyper::Client<C>,
    url: Url,
    snapshot_checksum: &[u8],
) -> anyhow::Result<()>
where
    C: Connect + Clone + Send + Sync + 'static,
{
    info!("Validating checksum...");
    let checksum_url = replace_extension_url(url, "sha256sum")?;
    let checksum_expected_file = client.get(checksum_url.as_str().try_into()?).await?;
    if !checksum_expected_file.status().is_success() {
        bail!("Unable to get the checksum file. Snapshot downloaded but not verified.");
    }

    let checksum_bytes = hyper::body::to_bytes(checksum_expected_file.into_body()).await?;
    // checksum file is hex-encoded with optionally trailing `- ` at the end. Take
    // only what's needed, i.e. encoded digest, for SHA256 it's 32 bytes.
    let checksum_expected = checksum_from_file(&checksum_bytes, Sha256::output_size())?;

    validate_checksum(&checksum_expected, snapshot_checksum)?;
    info!(
        "Snapshot checksum correct. {}",
        snapshot_checksum.encode_hex::<String>()
    );

    Ok(())
}

/// Creates regular checksum (raw bytes) from a checksum file with format:
/// `<hex-encoded checksum> -`
fn checksum_from_file(content: &[u8], digest_length: usize) -> anyhow::Result<Vec<u8>> {
    let checksum_hex = content
        .iter()
        .take(digest_length * 2)
        .copied()
        .collect::<Vec<u8>>();

    if checksum_hex.len() != digest_length * 2 {
        bail!(
            "Invalid content [{:?}] for provided digest length [{}]",
            content,
            digest_length
        );
    }

    Ok(Vec::<u8>::from_hex(&checksum_hex)?)
}

/// Validates checksum
/// * `expected_checksum` - expected checksum, e.g. provided along with the
///   snapshot file.
/// * `actual_checksum` - actual checksum, e.g. obtained by running a hasher
///   over a snapshot.
fn validate_checksum(expected_checksum: &[u8], actual_checksum: &[u8]) -> anyhow::Result<()> {
    if actual_checksum != expected_checksum {
        bail!(
            "Checksum incorrect. Downloaded snapshot checksum {}, expected checksum {}",
            actual_checksum.encode_hex::<String>(),
            expected_checksum.encode_hex::<String>(),
        );
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use std::{env::temp_dir, net::TcpListener};

    use anyhow::{ensure, Result};
    use axum::{routing::get_service, Router};
    use http::StatusCode;
    use quickcheck_macros::quickcheck;
    use rand::{distributions::Alphanumeric, Rng};
    use tempfile::TempDir;
    use tower_http::services::ServeDir;

    use super::*;

    #[test]
    fn checksum_from_file_test() {
        assert_eq!(
            checksum_from_file(b"00aaff -", 3).unwrap(),
            [0x00, 0xaa, 0xff]
        );
        assert_eq!(
            checksum_from_file(b"00aaff", 3).unwrap(),
            [0x00, 0xaa, 0xff]
        );

        assert!(checksum_from_file(b"00aaff -", 4).is_err());
        assert!(checksum_from_file(b"cthulhuu", 4).is_err());
    }

    #[test]
    fn validate_checksum_test() {
        assert!(validate_checksum(b"1234", b"1234").is_ok());
        assert!(validate_checksum(b"1234", b"1235").is_err());
    }

    #[test]
    fn filename_from_url_test() {
        let correct_cases = [
            ("https://cthulhu.org/necronomicon.txt", "necronomicon.txt"),
            (
                "https://cthulhu.org/necronomicon.txt?respect=yes",
                "necronomicon.txt",
            ),
            ("https://cthulhu.org/necro/nomicon", "nomicon"),
        ];

        correct_cases.iter().for_each(|case| {
            assert_eq!(
                filename_from_url(&Url::try_from(case.0).unwrap()).unwrap(),
                case.1
            )
        });

        let error_cases = [
            "https://cthulhu.org", // no resource
        ];

        error_cases
            .iter()
            .for_each(|case| assert!(filename_from_url(&Url::try_from(*case).unwrap()).is_err()));
    }

    #[quickcheck]
    fn test_normalize_filecoin_snapshot_name(filename: String) {
        _ = normalize_filecoin_snapshot_name("calibnet", &filename);
    }

    #[test]
    fn replace_extension_url_test() {
        let correct_cases = [
            (
                "https://cthulhu.org/necronomicon.txt",
                "pdf",
                "https://cthulhu.org/necronomicon.pdf",
            ),
            (
                "https://cthulhu.org/ne/cro/no/mi/con.txt",
                "pdf",
                "https://cthulhu.org/ne/cro/no/mi/con.pdf",
            ),
            (
                "https://cthulhu.org/necronomicon.txt?respect=yes",
                "pdf",
                "https://cthulhu.org/necronomicon.pdf?respect=yes",
            ),
        ];

        correct_cases.iter().for_each(|case| {
            assert_eq!(
                replace_extension_url(case.0.try_into().unwrap(), case.1).unwrap(),
                case.2.try_into().unwrap()
            )
        });

        let error_cases = [
            ("https://cthulhu.org", "pdf"),               // no resource
            ("https://cthulhu.org/necro/nomicon", "pdf"), // no extension
        ];

        error_cases.iter().for_each(|case| {
            assert!(replace_extension_url(case.0.try_into().unwrap(), case.1).is_err())
        });
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn download_with_aria2_test_wrong_checksum() -> Result<()> {
        if !is_github_action() && !is_aria2_installed() {
            return Ok(());
        }

        let (url, shutdown_tx, _, _data_dir) = serve_random_file()?;
        let r = download_with_aria2(
            &url,
            temp_dir().as_os_str().to_str().unwrap_or_default(),
            "test",
            "sha-256=f640a228f127a7ad7c3d7c8fa4a9e95c5a2eb8d32561905d97191178ab383a64",
        );
        ensure!(r.is_err());
        let err = r.unwrap_err().to_string();
        ensure!(err.contains("Checksum validation failed"));
        shutdown_tx.send(()).unwrap();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn download_with_aria2_test_good_checksum() -> Result<()> {
        if !is_github_action() && !is_aria2_installed() {
            return Ok(());
        }

        let (url, shutdown_tx, shasum, _data_dir) = serve_random_file()?;
        download_with_aria2(
            &url,
            temp_dir().as_os_str().to_str().unwrap_or_default(),
            "test",
            &format!("sha-256={}", hex::encode(shasum)),
        )?;
        shutdown_tx.send(()).unwrap();
        Ok(())
    }

    /// Serves a random file over HTTP.
    /// Returns:
    /// - url of the served file,
    /// - service channel,
    /// - expected SHA-256 of the file,
    /// - handle to the temporary directory in which the file is created.
    fn serve_random_file() -> Result<(
        String,
        tokio::sync::oneshot::Sender<()>,
        sha2::digest::Output<Sha256>,
        TempDir,
    )> {
        // Create temporary directory
        let temp_dir = tempfile::Builder::new()
            .tempdir()
            .expect("Failed to create temporary path");

        // Create random file with a random name and calculate its sha256sum
        let data: Vec<_> = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(100)
            .collect();
        let filename: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(10)
            .map(char::from)
            .collect();
        std::fs::write(temp_dir.path().join(&filename), &data).unwrap();
        let shasum = sha2::Sha256::digest(&data);

        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let url = format!("http://{}:{}/{filename}", addr.ip(), addr.port());
        let app = {
            let serve_dir = get_service(ServeDir::new(temp_dir.path())).handle_error(|_| async {
                (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong...")
            });
            Router::new().nest_service("/", serve_dir)
        };
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server = axum::Server::from_tcp(listener)?
            .serve(app.into_make_service())
            .with_graceful_shutdown(async {
                shutdown_rx.await.ok();
            });
        tokio::spawn(server);
        Ok((url, shutdown_tx, shasum, temp_dir))
    }

    fn is_github_action() -> bool {
        // https://docs.github.com/en/actions/learn-github-actions/environment-variables#default-environment-variables
        std::env::var("GITHUB_ACTION").is_ok()
    }
}
