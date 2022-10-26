// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::{Config, SnapshotFetchConfig};
use anyhow::bail;
use chrono::DateTime;
use forest_utils::io::TempFile;
use hex::{FromHex, ToHex};
use log::info;
use pbr::ProgressBar;
use reqwest::{Client, Response, Url};
use s3::Bucket;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    fs::{create_dir_all, File},
    io::{AsyncWriteExt, BufWriter},
};

use crate::cli::to_size_string;

/// Fetches snapshot from a trusted location and saves it to the given directory. Chain is inferred
/// from configuration.
pub(crate) async fn snapshot_fetch(
    snapshot_out_dir: &Path,
    config: Config,
) -> anyhow::Result<PathBuf> {
    match config.chain.name.to_lowercase().as_ref() {
        "mainnet" => snapshot_fetch_mainnet(snapshot_out_dir, &config.snapshot_fetch).await,
        "calibnet" => snapshot_fetch_calibnet(snapshot_out_dir, &config.snapshot_fetch).await,
        _ => Err(anyhow::anyhow!(
            "Fetch not supported for chain {}",
            config.chain.name,
        )),
    }
}

/// Fetches snapshot for `calibnet` from a default, trusted location. On success, the snapshot will be
/// saved in the given directory. In case of failure (e.g. connection interrupted) it will not be
/// removed.
async fn snapshot_fetch_calibnet(
    snapshot_out_dir: &Path,
    snapshot_fetch_config: &SnapshotFetchConfig,
) -> anyhow::Result<PathBuf> {
    let name = &snapshot_fetch_config.calibnet.bucket_name;
    let region = &snapshot_fetch_config.calibnet.region;
    let bucket = Bucket::new_public(name, region.parse()?)?;

    // Grab contents of the bucket
    let bucket_contents = bucket
        .list(
            snapshot_fetch_config.calibnet.path.clone(),
            Some("/".to_string()),
        )
        .await?;

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
    // It'd be better to use the bucket directly with `get_object_stream`, but at the time
    // of writing this code the Stream API is a bit lacking, making adding a progress bar a pain.
    // https://github.com/durch/rust-s3/issues/275
    let client = Client::new();
    let snapshot_spaces_url = &snapshot_fetch_config.calibnet.snapshot_spaces_url;
    let calibnet_path = &snapshot_fetch_config.calibnet.path;
    let url = snapshot_spaces_url.join(calibnet_path)?.join(filename)?;

    let snapshot_response = client.get(url.clone()).send().await?;
    let total_size = last_modified.size;
    download_snapshot_and_validate_checksum(
        client,
        url,
        &snapshot_path,
        snapshot_response,
        total_size,
    )
    .await?;

    Ok(snapshot_path)
}

/// Fetches snapshot for `mainnet` from a default, trusted location. On success, the snapshot will be
/// saved in the given directory. In case of failure (e.g. checksum verification fiasco) it will
/// not be removed.
async fn snapshot_fetch_mainnet(
    snapshot_out_dir: &Path,
    snapshot_fetch_config: &SnapshotFetchConfig,
) -> anyhow::Result<PathBuf> {
    let client = Client::new();

    let snapshot_url = snapshot_fetch_config.mainnet.snapshot_url.clone();
    let snapshot_response = client.get(snapshot_url.clone()).send().await?;

    // Use the redirect if available.
    let snapshot_url = match snapshot_response
        .headers()
        .get("x-amz-website-redirect-location")
    {
        Some(url) => url.to_str()?.try_into()?,
        None => snapshot_url,
    };

    let total_size = snapshot_response
        .content_length()
        .ok_or_else(|| anyhow::anyhow!("Couldn't retrieve content length"))?;

    // Grab the snapshot file name
    let snapshot_name = filename_from_url(&snapshot_url)?;
    // Create requested directory tree to store the snapshot
    create_dir_all(snapshot_out_dir).await?;
    let snapshot_path = snapshot_out_dir.join(&snapshot_name);

    download_snapshot_and_validate_checksum(
        client,
        snapshot_url,
        &snapshot_path,
        snapshot_response,
        total_size,
    )
    .await?;

    Ok(snapshot_path)
}

/// Downloads snapshot to a file with a progress bar. Returns the digest of the downloaded file.
async fn download_snapshot_and_validate_checksum(
    client: Client,
    url: Url,
    snapshot_path: &Path,
    snapshot_response: Response,
    total_size: u64,
) -> anyhow::Result<()> {
    info!(
        "Snapshot will be downloaded to {} ({})",
        snapshot_path.display(),
        to_size_string(&total_size.into())?
    );

    let mut progress_bar = ProgressBar::new(total_size);
    progress_bar.message("Downloading snapshot ");
    progress_bar.set_max_refresh_rate(Some(Duration::from_millis(500)));
    progress_bar.set_units(pbr::Units::Bytes);

    let snapshot_file_tmp = TempFile::new(snapshot_path.with_extension("car.tmp"));
    let file = File::create(snapshot_file_tmp.path()).await?;
    let mut writer = BufWriter::new(file);
    let mut downloaded: u64 = 0;
    let mut stream = snapshot_response.bytes_stream();

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

/// Fetches the relevant checksum for the snapshot, compares it with the result one. Fails if they
/// don't match. The checksum is expected to be located in the same location as the snapshot but
/// with a `.sha256sum` extension.
async fn fetch_checksum_and_validate(
    client: Client,
    url: Url,
    snapshot_checksum: &[u8],
) -> anyhow::Result<()> {
    info!("Validating checksum...");
    let checksum_url = replace_extension_url(url, "sha256sum")?;
    let checksum_expected_file = client.get(checksum_url).send().await?;
    if !checksum_expected_file.status().is_success() {
        bail!("Unable to get the checksum file. Snapshot downloaded but not verified.");
    }

    // checksum file is hex-encoded with optionally trailing `- ` at the end. Take only what's needed, i.e.
    // encoded digest, for SHA256 it's 32 bytes.
    let checksum_expected = checksum_from_file(
        &checksum_expected_file.bytes().await?,
        Sha256::output_size(),
    )?;

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
/// * `expected_checksum` - expected checksum, e.g. provided along with the snapshot file.
/// * `actual_checksum` - actual checksum, e.g. obtained by running a hasher over a snapshot.
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
}
