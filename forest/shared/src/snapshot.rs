// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! We occasionally fetch snapshots and store them locally, in
//! `snapshot_dir` This contains utilities for fetching, enumerating,
//! and deleting snapshots in `snapshot_dir`. Users should *not* call
//! multiple operations on `snapshot_dir` from different threads.
//!
//! # Storing snapshots
//! Snapshots are stored compressed as
//! `<snapshot_dir>/<slug>/<height>_<datetime>.car.zst`
//! E.g `<snapshot_dir>/mainnet/64050_2022_11_24T00_00_00Z.car.zst`
//!
//! # Concepts
//! Other modules should *not* have to concern themselves with filename parsing
//! etc.
//!
//! # Future work
//! - Be resilient to changes in snapshot filename format upstream
//! - Keep a register/machine readable db of snapshots, don't store metadata in
//!   filenames
//! - Mutual exclusion on snapshot_dir, e.g with `flock`

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context as _};
use itertools::Itertools;
use tempfile::NamedTempFile;
use url::Url;

/// List all snapshots in `snapshot_dir`
///
/// Note this function makes blocking syscalls, and should not be called
/// from an async context. Use [tokio::task::spawn_blocking] if needed.
pub fn list(snapshot_dir: &Path) -> anyhow::Result<Vec<Snapshot>> {
    walkdir::WalkDir::new(snapshot_dir)
        .into_iter()
        .filter_ok(|entry| entry.file_type().is_file())
        .map_ok(|entry| {
            let path = entry.path().to_path_buf();
            let slug = entry
                .path()
                .parent()
                .and_then(|parent| parent.to_str().map(String::from))
                .context("invalid or absent slug in snapshot directory")?;
            let metadata = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse().ok())
                .context("invalid filename for snapshot in snapshot directory")?;
            anyhow::Ok(Snapshot {
                slug,
                metadata,
                path,
            })
        })
        // This short-circuits our errors, but there's a case to be made for not
        // falling over if our directory structure doesn't look right
        .collect::<Result<Result<Vec<_>, _>, _>>()
        .context("couldn't walk snapshot directory")?
}

/// Remove and recreate `snapshot_dir`
///
/// Note this function makes blocking syscalls, and should not be called
/// from an async context. Use [tokio::task::spawn_blocking] if needed.
pub fn clean(snapshot_dir: &Path) -> anyhow::Result<()> {
    std::fs::remove_dir_all(snapshot_dir).context("error removing snapshot directory")?;
    std::fs::create_dir_all(snapshot_dir).context("error recreating snapshot dir")?;
    todo!()
}

/// Fetch a snapshot
pub async fn fetch(
    snapshot_dir: &Path,
    slug: &str,
    client: &reqwest::Client,
    stable_url: Url,
    progress_bar: &indicatif::ProgressBar,
) -> anyhow::Result<PathBuf> {
    tokio::fs::create_dir_all(snapshot_dir.join(slug)).await?;
    let (_meta, file_url, _file_name, file_len) = peek_snapshot(client, stable_url).await?;
    progress_bar.set_length(file_len);
    match download_to_temp(client, file_url, progress_bar).await {
        Ok((path, final_url)) => match metadata_and_filename(&final_url) {
            Ok((_meta, file_name)) => {
                let final_path = snapshot_dir.join(slug).join(file_name);
                tokio::fs::rename(path, &final_path)
                    .await
                    .context("couldn't move download to final location")?;
                // TODO(aatifsyed): we've probably leaked download here
                Ok(final_path)
            }
            // we downloaded the wrong thing
            Err(err) => {
                // TODO(aatifsyed): we've maybe leaked download here
                let _ = tokio::fs::remove_file(path).await;
                Err(err)
            }
        },
        // something went wrong with the download
        Err(err) => {
            progress_bar.abandon();
            Err(err)
        }
    }
}

/// Takes a stable url like `https://snapshots.calibrationnet.filops.net/minimal/latest`, and follows it to get
/// - The metadata of the file to download, inferred from a conventional name.
/// - The url of the actual file to download (this prevents races where the
///   stable url switches its target during an operation).
/// - The name of the file.
/// - The length the file to download
async fn peek_snapshot(
    client: &reqwest::Client,
    url: impl reqwest::IntoUrl,
) -> anyhow::Result<(SnapshotMetadata, Url, String, u64)> {
    // issue an actual GET, so the content length will be of the body
    // (we never actually fetch the body)
    // if we issue a HEAD, the content-length will be zero for redirect URLs
    // (this is a bug, maybe in reqwest - HEAD _should_ give us the length)
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()
        .context("server returned an error response")?;
    let length = response
        .content_length()
        .context("no content-length header")?;
    let (metadata, filename) = metadata_and_filename(response.url())?;
    Ok((metadata, response.url().clone(), filename, length))
}

/// Download the file at `url` returning
/// - The path to the downloaded file
/// - The url of the download file (in case e.g redirects were followed)
async fn download_to_temp(
    client: &reqwest::Client,
    url: Url,
    progress_bar: &indicatif::ProgressBar,
) -> anyhow::Result<(PathBuf, Url)> {
    use futures::TryStreamExt as _;
    use tap::Pipe as _;
    let response = client
        .get(url)
        .send()
        .await?
        .error_for_status()
        .context("server returned an error response")?;
    let url = response.url().clone();
    let mut src = response
        .bytes_stream()
        .map_err(|reqwest_error| std::io::Error::new(std::io::ErrorKind::Other, reqwest_error))
        .pipe(tokio_util::io::StreamReader::new)
        .pipe(|reader| progress_bar.wrap_async_read(reader));
    let (std_file, path) =
        tokio::task::spawn_blocking(|| anyhow::Ok(NamedTempFile::new()?.keep()?))
            .await
            .expect("NamedTempFile::new doesn't panic, and we didn't cancel/abort this task")
            .context("couldn't create temporary file for download")?;
    let mut dst = tokio::fs::File::from_std(std_file);
    match tokio::io::copy(&mut src, &mut dst).await {
        Ok(_) => Ok((path, url)),
        Err(e) => {
            // TODO(aatifsyed): we've maybe leaked the download here
            let _ = tokio::fs::remove_file(path).await;
            Err(e).context("couldn't download to file")
        }
    }
}

pub struct Snapshot {
    pub slug: String,
    pub metadata: SnapshotMetadata,
    pub path: PathBuf,
}

pub fn enter_nom<'a, T>(
    mut parser: impl nom::Parser<&'a str, T, nom::error::Error<&'a str>>,
    input: &'a str,
) -> anyhow::Result<T> {
    let (rem, t) = parser
        .parse(input)
        .map_err(|e| anyhow!("Parser error: {e}"))?;
    if !rem.is_empty() {
        bail!("Unexpected trailing input: {rem}")
    }
    Ok(t)
}

fn metadata_and_filename(url: &Url) -> anyhow::Result<(SnapshotMetadata, String)> {
    let name = url
        .path_segments()
        .context("url has no path")?
        .last()
        .context("url has no segments")?
        .to_string();
    Ok((name.parse().context("unexpected format for name")?, name))
}

/// The information contained in the filename of compressed snapshots served
/// by `filops`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotMetadata {
    pub height: i64,
    pub datetime: chrono::NaiveDateTime,
}

#[test]
fn parse() {
    let metadata = SnapshotMetadata::from_str("64050_2022_11_24T00_00_00Z.car.zst").unwrap();
    assert_eq!(
        SnapshotMetadata {
            height: 64050,
            datetime: chrono::NaiveDateTime::from_str("2022-11-24T00:00:00").unwrap()
        },
        metadata,
    )
}

/// Parse a number using its [`FromStr`] implementation.
fn number<T>(input: &str) -> nom::IResult<&str, T>
where
    T: FromStr,
{
    use nom::{
        character::complete::digit1,
        combinator::{map_res, recognize},
        multi::many1,
    };
    map_res(recognize(many1(digit1)), T::from_str)(input)
}

impl FromStr for SnapshotMetadata {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use nom::bytes::complete::tag;
        let (height, _, year, _, month, _, day, _, hour, _, min, _, sec, _, _) = enter_nom(
            nom::sequence::tuple((
                number, // height
                tag("_"),
                number, // year
                tag("_"),
                number, // month
                tag("_"),
                number, // day
                tag("T"),
                number, // hour
                tag("_"),
                number, // minute
                tag("_"),
                number, // second,
                tag("Z"),
                tag(".car.zst"),
            )),
            s,
        )?;
        let datetime = chrono::NaiveDateTime::new(
            chrono::NaiveDate::from_ymd_opt(year, month, day).context("invalid date")?,
            chrono::NaiveTime::from_hms_opt(hour, min, sec).context("invalid time")?,
        );
        Ok(Self { height, datetime })
    }
}
