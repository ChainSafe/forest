// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! We occasionally fetch _compressed_ snapshots of `chain`s from `vendor`s
//! and store them locally, in the `snapshot_directory`. See
//! [crate::cli::Config::snapshot_directory]. The snapshots live at
//! `stable_url`s - see [stable_url]'s source for the supported chains and vendors.
//!
//! This module contains utilities for fetching, enumerating, interning (accepting
//! from other locations) snapshots. Users should be aware that operations on the
//! snapshot directory may race.
//!
//! # Implementation
//! The snapshot store is actually a single directory, containing a flat store
//! of files. Files come in pairs:
//! - The actual data _blob_, named e.g `foo.car.zst`
//! - A _metadata_ file, named e.g `foo.car.zst.forestmetadata.json`. See
//!   [METADATA_FILE_SUFFIX]
//!
//! All files are ultimately interned by [intern_and_create_metadata], whether from
//! the cli, or from the web.
//!
//! We assign no semantic meaning to the filenames other than the blob/metadata
//! distinction - all that matters is that they are unique.
//!
//! ## Concepts
//! Other modules should *not* have to concern themselves with filename parsing
//! etc.
//!
//! ## Future work
//! - Be resilient to changes in snapshot filename format upstream
//! - Keep a register/machine readable db of snapshots, don't store metadata in
//!   parallel
//! - Mutual exclusion on snapshot_dir, e.g with `flock`

use std::{
    collections::BTreeSet,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context as _};
use chrono::{NaiveDate, NaiveTime};
use forest_networks::NetworkChain;
use itertools::Itertools as _;
use serde::{Deserialize, Serialize};
use tap::Tap as _;
use tempfile::NamedTempFile;
use tracing::warn;
use url::Url;

const METADATA_FILE_SUFFIX: &str = ".metadata.json";

/// Fetch a snapshot.
///
/// See [module documentation](mod@self) for more.
pub async fn fetch(
    snapshot_dir: &Path,
    chain: &NetworkChain,
    vendor: &str,
    client: &reqwest::Client,
    progress_bar: &indicatif::ProgressBar,
) -> anyhow::Result<PathBuf> {
    let stable_url = stable_url(vendor, chain)
        .with_context(|| format!("unsupported chain `{chain}` or vendor `{vendor}`"))?;

    fetch_impl(
        snapshot_dir,
        stable_url,
        chain,
        vendor,
        client,
        progress_bar,
    )
    .await
}

/// List all paths to files and their metadata in `snapshot_directory`. Will
/// return [`Ok(Vec::new())`] if `snapshot_directory` does not exist.
///
/// Users can freely delete the path to the blob - corresponding metadata will
/// be cleaned up in the next call to [list], but this should be regarded as an
/// implementation detail.
///
/// See [module documentation](mod@self) for more.
///
/// Note this function makes blocking syscalls, and should not be called
/// from an async context. Use [tokio::task::spawn_blocking] if needed.
pub fn list(snapshot_directory: &Path) -> anyhow::Result<Vec<(PathBuf, SnapshotMetadata)>> {
    if !snapshot_directory.exists() {
        return Ok(Vec::new());
    }
    // Get all the file paths
    let mut paths = walkdir::WalkDir::new(snapshot_directory)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_ok(|entry| entry.path().is_file())
        .map_ok(|entry| match entry.path().to_str() {
            // prefix operation on strings are easier, so convert to those
            Some(s) => Some(String::from(s)),
            None => {
                warn!(path = %entry.path().display(), "ignored non-utf8 file in snapshot directory");
                None
            }
        })
        .flatten_ok()
        .collect::<Result<BTreeSet<_>, _>>()
        .context("couldn't enumerate paths in snapshot directory")?;

    // Sort them into pairs
    let mut blobs_and_metadata = Vec::new();
    while let Some(path) = paths.pop_first() {
        match path.strip_suffix(METADATA_FILE_SUFFIX) {
            Some(blob_path) => {
                // we've popped the metadata file, try and pop the blob path
                let blob_was_present = paths.remove(blob_path);
                match blob_was_present {
                    false => {
                        warn!(%path, "deleting metadata without corresponding blob");
                        let _ = std::fs::remove_file(path);
                    }
                    true => {
                        let blob_path = PathBuf::from(blob_path);
                        let metadata_path = PathBuf::from(path);
                        blobs_and_metadata.push((blob_path, metadata_path));
                    }
                }
            }
            None => {
                // this is the blob path
                let metadata_path = format!("{path}{METADATA_FILE_SUFFIX}");
                let blob_path = path;
                let metadata_was_present = paths.remove(&metadata_path);
                match metadata_was_present {
                    false => {
                        warn!(path = %blob_path, "ignored blob without corresponding metadata")
                    }
                    true => blobs_and_metadata.push((blob_path.into(), metadata_path.into())),
                }
            }
        }
    }

    Ok(blobs_and_metadata
        .into_iter()
        .flat_map(|(blob, metadata)| {
            std::fs::read_to_string(&metadata)
                .map_err(|_| warn!(path = ?metadata, "ignoring unreadable metadata file"))
                .and_then(|s| {
                    serde_json::from_str(&s).map_err(
                        |_| warn!(path = ?metadata, "ignoring invalid format for metadata file"),
                    )
                })
                .map(|metadata| (blob, metadata))
        })
        .collect())
}

/// `file` is a snapshot file with a conventional name.
/// This function will move `file` into `snapshot_dir`, adding an appropriate metadata
/// file to allow it to be recognised in [list].
pub async fn intern(
    snapshot_dir: &Path,
    file: &Path,
    chain: &NetworkChain,
    vendor: &str,
) -> anyhow::Result<PathBuf> {
    let (height, date, time) = file
        .file_name()
        .and_then(OsStr::to_str)
        .context("non-utf8 or missing filename")
        .and_then(parse::parse_filename)
        .context("invalid filename")?;

    intern_and_create_metadata(
        snapshot_dir,
        file,
        height,
        date,
        time,
        chain,
        vendor,
        None,
        None,
    )
    .await
}

/// Metadata about a snapshot blob
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotMetadata {
    // We use an enum to handle forward-incompatible changes in the future
    V1 {
        height: i64,
        date: NaiveDate,
        // The `forest` vendor doesn't include time
        time: Option<chrono::NaiveTime>,
        chain: String,
        vendor: String,
        // The stable url used
        source_url: Option<String>,
        fetched_url: Option<String>,
    },
}

/// Moves the file `blob` into `snapshot_dir`, and creates a metadata file for it.
///
/// This will preserve the filename of `blob`.
/// Does not clean up `blob` on failure.
#[allow(clippy::too_many_arguments)]
async fn intern_and_create_metadata(
    snapshot_dir: &Path,
    blob: &Path,
    height: i64,
    date: NaiveDate,
    time: Option<NaiveTime>,
    chain: &NetworkChain,
    vendor: &str,
    source_url: Option<String>,
    fetched_url: Option<String>,
) -> anyhow::Result<PathBuf> {
    let metadata_contents = serde_json::to_string(&SnapshotMetadata::V1 {
        height,
        date,
        time,
        chain: chain.to_string(),
        vendor: String::from(vendor),
        source_url,
        fetched_url,
    })
    .expect("serialization of metadata shouldn't fail");
    let blob_name = blob.file_name().context("no filename")?.to_os_string();
    let new_blob_path = snapshot_dir.join(&blob_name);
    let metadata_path = snapshot_dir.join(blob_name.tap_mut(|it| it.push(METADATA_FILE_SUFFIX)));

    tokio::fs::write(metadata_path, metadata_contents)
        .await
        .context("couldn't write metadata file")?;

    tokio::fs::rename(blob, &new_blob_path)
        .await
        .context("couldn't move blob to snapshot directory")?;

    Ok(new_blob_path)
}

/// Unit-testable implementation of [fetch]
async fn fetch_impl(
    snapshot_dir: &Path,
    stable_url: &str,
    chain: &NetworkChain,
    vendor: &str,
    client: &reqwest::Client,
    progress_bar: &indicatif::ProgressBar,
) -> anyhow::Result<PathBuf> {
    tokio::fs::create_dir_all(snapshot_dir).await?;

    let (height, date, time, actual_url, file_len) = peek_snapshot(client, stable_url).await?;

    progress_bar.set_length(file_len);

    match download_to_temp(client, actual_url.clone(), progress_bar).await {
        Ok((path, final_url)) if final_url == actual_url => {
            intern_and_create_metadata(
                snapshot_dir,
                &path,
                height,
                date,
                time,
                chain,
                vendor,
                Some(String::from(stable_url)),
                Some(String::from(actual_url)),
            )
            .await
        }
        Ok((path, _)) => {
            let _ = tokio::fs::remove_file(path).await;
            bail!("mismatch between metadata and downloaded file");
        }
        // something went wrong with the download
        Err(err) => {
            progress_bar.abandon();
            Err(err)
        }
    }
}

/// Takes a stable url like `https://snapshots.calibrationnet.filops.net/minimal/latest`, and follows it to get
/// - Metadata inferred from the (conventional) filename
///   - height
///   - date
///   - (optional) time
/// - The url of the actual file to download (this prevents races where the
///   stable url switches its target during an operation).
/// - The length the file to download
async fn peek_snapshot(
    client: &reqwest::Client,
    stable_url: &str,
) -> anyhow::Result<(i64, NaiveDate, Option<NaiveTime>, Url, u64)> {
    // issue an actual GET, so the content length will be of the body
    // (we never actually fetch the body)
    // if we issue a HEAD, the content-length will be zero for redirect URLs
    // (this is a bug, maybe in reqwest - HEAD _should_ give us the length)
    // (maybe because the stable URLs are all double-redirects? 301 -> 302 -> 200)
    let response = client
        .get(stable_url)
        .send()
        .await?
        .error_for_status()
        .context("server returned an error response")?;
    let length = response
        .content_length()
        .context("no content-length header")?;
    // could also look at Content-Disposition, but that's even more finicky
    let filename = response
        .url()
        .path_segments()
        .context("url has no path")?
        .last()
        .context("url has no segments")?;
    let (height, date, time) =
        parse::parse_filename(filename).context("unexpected filename format on remote server")?;
    Ok((height, date, time, response.url().clone(), length))
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

const FOREST_MAINNET_COMPRESSED: &str =
    "https://forest.chainsafe.io/mainnet/snapshot-latest.car.zst";
const FOREST_CALIBNET_COMPRESSED: &str =
    "https://forest.chainsafe.io/calibnet/snapshot-latest.car.zst";
const FILOPS_MAINNET_COMPRESSED: &str = "https://snapshots.mainnet.filops.net/minimal/latest.zst";
const FILOPS_CALIBNET_COMPRESSED: &str =
    "https://snapshots.calibrationnet.filops.net/minimal/latest.zst";

fn stable_url(vendor: &str, chain: &NetworkChain) -> Option<&'static str> {
    match (vendor, chain) {
        ("forest", NetworkChain::Mainnet) => Some(FOREST_MAINNET_COMPRESSED),
        ("forest", NetworkChain::Calibnet) => Some(FOREST_CALIBNET_COMPRESSED),
        ("filops", NetworkChain::Mainnet) => Some(FILOPS_MAINNET_COMPRESSED),
        ("filops", NetworkChain::Calibnet) => Some(FILOPS_CALIBNET_COMPRESSED),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use httptest::{matchers::request::method_path, responders::status_code, Expectation};
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn parse_filename() {
        fn make_expected(
            year: i32,
            month: u32,
            day: u32,
            hms: impl Into<Option<(u32, u32, u32)>>,
            height: i64,
        ) -> (i64, NaiveDate, Option<NaiveTime>) {
            (
                height,
                NaiveDate::from_ymd_opt(year, month, day).unwrap(),
                hms.into()
                    .map(|(h, m, s)| NaiveTime::from_hms_opt(h, m, s).unwrap()),
            )
        }
        for (input, expected) in [
            (
                "forest_snapshot_mainnet_2023-05-30_height_2905376.car.zst",
                make_expected(2023, 5, 30, None, 2905376),
            ),
            (
                "forest_snapshot_calibnet_2023-05-30_height_604419.car.zst",
                make_expected(2023, 5, 30, None, 604419),
            ),
            (
                "2905920_2023_05_30T22_00_00Z.car.zst",
                make_expected(2023, 5, 30, (22, 0, 0), 2905920),
            ),
            (
                "605520_2023_05_31T00_13_00Z.car.zst",
                make_expected(2023, 5, 31, (0, 13, 0), 605520),
            ),
        ] {
            assert_eq!(expected, parse::parse_filename(input).unwrap());
        }
    }

    #[tokio::test]
    async fn test_fetch_and_list() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let server = httptest::Server::run();
        server.expect(
            Expectation::matching(method_path("GET", "/stable")).respond_with(
                status_code(301).insert_header("Location", "/2905920_2023_05_30T22_00_00Z.car.zst"),
            ),
        );
        server.expect(
            Expectation::matching(method_path("GET", "/2905920_2023_05_30T22_00_00Z.car.zst"))
                .times(1..)
                .respond_with(status_code(200)),
        );
        fetch_impl(
            temp_dir.path(),
            &server.url_str("/stable"),
            &NetworkChain::Mainnet,
            "testvendor",
            &reqwest::Client::new(),
            &indicatif::ProgressBar::hidden(),
        )
        .await?;
        let (_path, metadata) = list(temp_dir.path())?.into_iter().exactly_one()?;
        assert_eq!(
            SnapshotMetadata::V1 {
                height: 2905920,
                date: NaiveDate::from_ymd_opt(2023, 5, 30).unwrap(),
                time: Some(NaiveTime::from_hms_opt(22, 0, 0)).unwrap(),
                chain: String::from("mainnet"),
                vendor: String::from("testvendor"),
                source_url: Some(server.url_str("/stable")),
                fetched_url: Some(server.url_str("/2905920_2023_05_30T22_00_00Z.car.zst"))
            },
            metadata
        );
        Ok(())
    }
}

/// Filops and forest store metadata in the filename, in a conventional format.
/// [parse_filename] is able to parse the contained metadata.
mod parse {
    use std::str::FromStr;

    use anyhow::{anyhow, bail};
    use chrono::{NaiveDate, NaiveTime};
    use nom::{
        branch::alt,
        bytes::complete::tag,
        character::complete::digit1,
        combinator::{map_res, recognize},
        error::ErrorKind,
        error_position,
        multi::many1,
        sequence::tuple,
        Err, Parser as _,
    };

    /// Parse a number using its [`FromStr`] implementation.
    fn number<T>(input: &str) -> nom::IResult<&str, T>
    where
        T: FromStr,
    {
        map_res(recognize(many1(digit1)), T::from_str)(input)
    }

    /// Create a parser for `YYYY-MM-DD` etc
    fn ymd(separator: &str) -> impl Fn(&str) -> nom::IResult<&str, NaiveDate> + '_ {
        move |input| {
            let (rest, (year, _, month, _, day)) =
                tuple((number, tag(separator), number, tag(separator), number))(input)?;
            match NaiveDate::from_ymd_opt(year, month, day) {
                Some(date) => Ok((rest, date)),
                None => Err(Err::Error(error_position!(input, ErrorKind::Verify))),
            }
        }
    }

    /// Create a parser for `HH_MM_SS` etc
    fn hms(separator: &str) -> impl Fn(&str) -> nom::IResult<&str, NaiveTime> + '_ {
        move |input| {
            let (rest, (hour, _, minute, _, second)) =
                tuple((number, tag(separator), number, tag(separator), number))(input)?;
            match NaiveTime::from_hms_opt(hour, minute, second) {
                Some(date) => Ok((rest, date)),
                None => Err(Err::Error(error_position!(input, ErrorKind::Verify))),
            }
        }
    }

    fn forest(input: &str) -> nom::IResult<&str, (NaiveDate, i64)> {
        let (rest, (_, date, _, height, _)) = tuple((
            alt((
                tag("forest_snapshot_mainnet_"),
                tag("forest_snapshot_calibnet_"),
            )),
            ymd("-"),
            tag("_height_"),
            number,
            tag(".car.zst"),
        ))(input)?;
        Ok((rest, (date, height)))
    }

    fn filops(input: &str) -> nom::IResult<&str, (i64, NaiveDate, NaiveTime)> {
        let (rest, (height, _, date, _, time, _)) = tuple((
            number,
            tag("_"),
            ymd("_"),
            tag("T"),
            hms("_"),
            tag("Z.car.zst"),
        ))(input)?;
        Ok((rest, (height, date, time)))
    }

    fn _parse_filename(input: &str) -> nom::IResult<&str, (i64, NaiveDate, Option<NaiveTime>)> {
        alt((
            forest.map(|(date, height)| (height, date, None)),
            filops.map(|(height, date, time)| (height, date, Some(time))),
        ))(input)
    }

    fn enter_nom<'a, T>(
        mut parser: impl nom::Parser<&'a str, T, nom::error::Error<&'a str>>,
        input: &'a str,
    ) -> anyhow::Result<T> {
        let (rest, t) = parser
            .parse(input)
            .map_err(|e| anyhow!("Parser error: {e}"))?;
        if !rest.is_empty() {
            bail!("Unexpected trailing input: {rest}")
        }
        Ok(t)
    }

    pub fn parse_filename(input: &str) -> anyhow::Result<(i64, NaiveDate, Option<NaiveTime>)> {
        enter_nom(_parse_filename, input)
    }
}
