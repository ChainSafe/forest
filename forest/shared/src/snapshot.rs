// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! We occasionally fetch _compressed_ snapshots of `chain`s from `vendor`s
//! and store them locally, in the `snapshot_dir`. See
//! [`crate::cli::Config::snapshot_directory`]. The snapshots live at
//! `stable_url`s - see [`stable_url`]'s source for the supported chains and vendors.
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
//!   [`SNAPSHOT_METADATA_FILE_SUFFIX`]
//!
//! All files are ultimately interned by [`intern_and_create_metadata`], whether from
//! the CLI, or from the web.
//!
//! We assign no semantic meaning to the filenames in the snapshot directory,
//! other than the blob/metadata distinction - all that matters is that they are unique.
//!
//! ## `Aria2`
//! We prefer to download files using `aria2c`, falling back to making the request
//! ourselves.
//!
//! We treat a sub-folder of `snapshot_dir` as our interface with `aria2`,
//! downloading and retrieving files from it as appropriate.
//!
//! ## Concepts
//! Other modules should *not* have to concern themselves with filename parsing
//! etc.
//!
//! ## Future work
//! - Be resilient to changes in snapshot filename format upstream
//! - Keep a register/machine readable db of snapshots, don't store metadata in
//!   parallel
//! - Mutual exclusion on `snapshot_dir`, e.g with `flock`

use std::{
    collections::BTreeSet,
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context as _};
use chrono::{NaiveDate, NaiveTime};
use forest_networks::NetworkChain;
use forest_utils::io::progress_bar::downloading_style;
use futures::future::join_all;
use itertools::Itertools as _;
use serde::{Deserialize, Serialize};
use tap::Tap as _;
use tempfile::NamedTempFile;
use tracing::{debug, info, warn};
use url::Url;

const SNAPSHOT_METADATA_FILE_SUFFIX: &str = ".metadata.json";
const ARIA2C_METADATA_FILE_SUFFIX: &str = ".aria";

/// List all paths to files and their metadata in `snapshot_dir`.
/// Returns [`Ok(Vec::new())`] if `snapshot_dir` does not exist.
///
/// Users can freely delete the path to the blob - corresponding metadata will
/// be cleaned up in the next call to [list], but this should be regarded as an
/// implementation detail.
///
/// See [module documentation](mod@self) for more.
///
/// Note this function makes blocking syscalls, and should not be called
/// from an async context. Use [`tokio::task::spawn_blocking`] if needed.
pub fn list(snapshot_dir: &Path) -> anyhow::Result<Vec<(PathBuf, SnapshotMetadata)>> {
    if !snapshot_dir.exists() {
        return Ok(Vec::new());
    }
    let Partitioned {
        main_and_meta,
        orphaned_meta,
        orphaned_main,
    } = partition_by_suffix(snapshot_dir, SNAPSHOT_METADATA_FILE_SUFFIX)
        .context("couldn't enumerate snapshots in snapshot directory")?;
    for meta in orphaned_meta {
        debug!(path = %meta.display(), "deleting metadata without corresponding blob");
        let _ = std::fs::remove_file(meta);
    }
    for main in orphaned_main {
        debug!(path = %main.display(), "ignoring snapshots without metadata files")
    }

    Ok(main_and_meta
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

/// Fetch a snapshot with `aria2c`, falling back to our own HTTP client.
/// This may draw to stdout.
///
/// See [module documentation](mod@self) for more.
pub async fn fetch(
    snapshot_dir: &Path,
    chain: &NetworkChain,
    vendor: &str,
) -> anyhow::Result<PathBuf> {
    let stable_url = stable_url(vendor, chain)?;

    let aria2c_dir = snapshot_dir.join("aria2c");
    match download_aria2c(&aria2c_dir, stable_url).await {
        Ok(()) => intern_from_aria2c_dir(aria2c_dir, snapshot_dir, chain, vendor).await,
        Err(AriaErr::CouldNotExec(reason)) => {
            warn!(%reason, "couldn't run aria2c. Falling back to conventional download, which will be much slower - consider installing aria2c.");
            let (path, url) = download_http(stable_url).await?;
            let (height, date, time) = parse::parse_url(&url)?;
            intern_and_create_metadata(
                snapshot_dir,
                &path,
                height,
                date,
                time,
                chain,
                vendor,
                Some(String::from(stable_url)),
                Some(String::from(url.as_str())),
            )
            .await
        }
        Err(AriaErr::Other(o)) => Err(o.context("downloading with aria2c failed")),
    }
}

/// `file` is a snapshot file with a conventional name.
/// This function will move `file` into `snapshot_dir`, adding an appropriate metadata
/// file to allow it to be recognized in [list].
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

/// Find all completed downloads in `aria2c_dir`, and intern them.
/// The files are expected to have a conventional name.
/// Returns the first file interned - we typically only expect one file to be interned.
async fn intern_from_aria2c_dir(
    aria2c_dir: PathBuf,
    snapshot_dir: &Path,
    chain: &NetworkChain,
    vendor: &str,
) -> Result<PathBuf, anyhow::Error> {
    // completed downloads are files without the .aria extension
    // this is pretty fragile (see below), but it's the closest we have
    // to an API with aria2 at the moment
    let Partitioned { orphaned_main, .. } = tokio::task::spawn_blocking(move || {
        partition_by_suffix(&aria2c_dir, ARIA2C_METADATA_FILE_SUFFIX)
            .context("couldn't read aria2c files in snapshot directory")
    })
    .await
    .expect("task panicked")?;

    // maybe there's garbage in `aria2c_dir` that didn't make sense to intern.
    // as long as we've interned _something_, we've probably done what the
    // user wanted.
    let (mut successes, failures) = join_all(
        orphaned_main
            .iter()
            .map(|file| intern(snapshot_dir, file, chain, vendor)),
    )
    .await
    .into_iter()
    .partition_result::<Vec<_>, Vec<_>, _, _>();

    match successes.is_empty() {
        // just report the first one
        false => Ok(successes.remove(0)),
        true => Err(failures
            .into_iter()
            .reduce(|acc, el| acc.context(el))
            .unwrap_or(anyhow!("couldn't find file that aria2c downloaded"))
            .context("couldn't intern file that aria2c downloaded")),
    }
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
    tokio::fs::create_dir_all(snapshot_dir).await?;
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
    let metadata_path =
        snapshot_dir.join(blob_name.tap_mut(|it| it.push(SNAPSHOT_METADATA_FILE_SUFFIX)));

    tokio::fs::write(metadata_path, metadata_contents)
        .await
        .context("couldn't write metadata file")?;

    match tokio::fs::rename(blob, &new_blob_path).await {
        Ok(()) => Ok(new_blob_path),
        // Can't rename across filesystems, so copy the bytes from disk to disk
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => tokio::fs::copy(blob, &new_blob_path)
            .await
            .map(|_| new_blob_path),
        Err(other) => Err(other),
    }
    .context("couldn't move blob to snapshot directory")
}

/// What would the size of the snapshot (in bytes) from the given `vendor` for this `chain` be?
pub async fn peek_num_bytes(vendor: &str, chain: &NetworkChain) -> anyhow::Result<u64> {
    let stable_url = stable_url(vendor, chain)?;
    // issue an actual GET, so the content length will be of the body
    // (we never actually fetch the body)
    // if we issue a HEAD, the content-length will be zero for redirect URLs
    // (this is a bug, maybe in reqwest - HEAD _should_ give us the length)
    // (probably because the stable URLs are all double-redirects 301 -> 302 -> 200)
    reqwest::get(stable_url)
        .await?
        .error_for_status()
        .context("server returned an error response")?
        .content_length()
        .context("no content-length header")
}

enum AriaErr {
    CouldNotExec(io::Error),
    Other(anyhow::Error),
}

/// Run `aria2c`, with inherited stdout and stderr (so output will be printed).
/// The file is downloaded to `aria2c_dir`, and should be retrieved separately.
async fn download_aria2c(aria2c_dir: &Path, url: &str) -> Result<(), AriaErr> {
    let exit_status = tokio::process::Command::new("aria2c")
        .args([
            "--continue=true",
            "--max-tries=0",
            // Download chunks concurrently, resulting in dramatically faster downloads
            "--split=5",
            "--max-connection-per-server=5",
            "--dir",
        ])
        .arg(aria2c_dir)
        .arg(url)
        .kill_on_drop(true) // allow cancellation
        .spawn() // defaults to inherited stdio
        .map_err(AriaErr::CouldNotExec)?
        .wait()
        .await
        .map_err(|it| AriaErr::Other(it.into()))?;

    match exit_status.success() {
        true => Ok(()),
        false => {
            let msg = exit_status
                .code()
                .map(|it| it.to_string())
                .unwrap_or_else(|| String::from("<killed>"));
            Err(AriaErr::Other(anyhow!("running aria2c failed: {msg}")))
        }
    }
}

/// Download the file at `url` with a private HTTP client, returning
/// - The path to the downloaded file
/// - The URL of the download file (in case e.g redirects were followed)
async fn download_http(url: &str) -> anyhow::Result<(PathBuf, Url)> {
    use futures::TryStreamExt as _;
    use tap::Pipe as _;
    let response = reqwest::get(url)
        .await?
        .error_for_status()
        .context("server returned an error response")?;
    let url = response.url().clone();
    info!(%url, "downloading snapshot");
    let progress_bar = indicatif::ProgressBar::new(0).with_style(downloading_style());
    if let Some(len) = response.content_length() {
        progress_bar.set_length(len)
    }
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
            let _ = tokio::fs::remove_file(path).await;
            Err(e).context("couldn't download to file")
        }
    }
}

/// It's common for programs to store ("main") files and metadata files side-by-side,
/// where the metadata file has a known suffix:
/// E.g with `aria2c`
/// - `forest_snapshot_calibnet_2023-06-06_height_624219.car.zst`
/// - `forest_snapshot_calibnet_2023-06-06_height_624219.car.zst.aria2`
///
/// Or with our snapshot logic:
/// - `foo.tmp`
/// - `foo.tmp.metadata.json`
///
/// This function looks at files which direct descendants of `directory`, returning:
/// - pairs of main files and metadata files
/// - orphaned main files
/// - orphaned metadata files
///
/// Paths are typically relative to `directory`.
/// Files with non-utf8 paths are [`warn`]-ed and ignored.
///
/// Note this function makes blocking syscalls, and should not be called
/// from an async context. Use [`tokio::task::spawn_blocking`] if needed.
fn partition_by_suffix(directory: &Path, metadata_suffix: &str) -> io::Result<Partitioned> {
    let mut main_and_meta = Vec::new();
    let mut orphaned_main = Vec::new();
    let mut orphaned_meta = Vec::new();

    // Get all the file paths
    let mut paths = walkdir::WalkDir::new(directory)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_ok(|entry| entry.path().is_file())
        .map_ok(|entry| match entry.path().to_str() {
            // prefix operation on strings are easier, so convert to those
            Some(s) => Some(String::from(s)),
            None => {
                warn!(path = %entry.path().display(), "ignored non-utf8 path");
                None
            }
        })
        .flatten_ok()
        .collect::<Result<BTreeSet<_>, _>>()?;

    // Do the partitioning
    while let Some(path) = paths.pop_first() {
        match path.strip_suffix(metadata_suffix) {
            Some(main) => {
                // we've popped the metadata file, try and pop the blob path
                let meta = PathBuf::from(&path);
                let main_was_present = paths.remove(main);
                match main_was_present {
                    false => orphaned_meta.push(meta),
                    true => main_and_meta.push((PathBuf::from(main), meta)),
                }
            }
            None => {
                // this is the blob path
                let meta = format!("{path}{metadata_suffix}");
                let main = PathBuf::from(path);
                let metadata_was_present = paths.remove(&meta);
                match metadata_was_present {
                    false => orphaned_main.push(main),
                    true => main_and_meta.push((main, PathBuf::from(meta))),
                }
            }
        }
    }

    Ok(Partitioned {
        main_and_meta,
        orphaned_meta,
        orphaned_main,
    })
}

struct Partitioned {
    main_and_meta: Vec<(PathBuf, PathBuf)>,
    orphaned_meta: Vec<PathBuf>,
    orphaned_main: Vec<PathBuf>,
}

const FOREST_MAINNET_COMPRESSED: &str =
    "https://forest.chainsafe.io/mainnet/snapshot-latest.car.zst";
const FOREST_CALIBNET_COMPRESSED: &str =
    "https://forest.chainsafe.io/calibnet/snapshot-latest.car.zst";
const FILOPS_MAINNET_COMPRESSED: &str = "https://snapshots.mainnet.filops.net/minimal/latest.zst";
const FILOPS_CALIBNET_COMPRESSED: &str =
    "https://snapshots.calibrationnet.filops.net/minimal/latest.zst";

fn stable_url(vendor: &str, chain: &NetworkChain) -> anyhow::Result<&'static str> {
    match (vendor, chain) {
        ("forest", NetworkChain::Mainnet) => Ok(FOREST_MAINNET_COMPRESSED),
        ("forest", NetworkChain::Calibnet) => Ok(FOREST_CALIBNET_COMPRESSED),
        ("filops", NetworkChain::Mainnet) => Ok(FILOPS_MAINNET_COMPRESSED),
        ("filops", NetworkChain::Calibnet) => Ok(FILOPS_CALIBNET_COMPRESSED),
        _ => bail!("unsupported vendor `{vendor}` or chain `{chain}`"),
    }
}

mod parse {
    //! Filops and forest store metadata in the filename, in a conventional format.
    //! [`parse_filename`] and [`parse_url`] are able to parse the contained metadata.

    use std::str::FromStr;

    use anyhow::{anyhow, bail, Context};
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
    use url::Url;

    pub fn parse_filename(input: &str) -> anyhow::Result<(i64, NaiveDate, Option<NaiveTime>)> {
        enter_nom(_parse_filename, input)
    }

    pub fn parse_url(url: &Url) -> anyhow::Result<(i64, NaiveDate, Option<NaiveTime>)> {
        let filename = url
            .path_segments()
            .context("url cannot be a base")?
            .last()
            .context("url has no path")?;
        parse_filename(filename)
    }

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

    #[test]
    fn test_parse_filename() {
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
            assert_eq!(expected, parse_filename(input).unwrap());
        }
    }
}
