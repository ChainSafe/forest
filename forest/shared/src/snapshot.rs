// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! We occasionally fetch snapshots and store them locally, in a
//! `snapshot_dir`.
//!
//! There is normally a different `snapshot_dir` for different chains - see
//! [super::cli::default_snapshot_dir].
//!
//! This module contains utilities for fetching,
//! enumerating, and deleting snapshots in `snapshot_dir`. Users should *not*
//! call multiple operations on `snapshot_dir` from different threads.
//!
//! # Storing snapshots
//! Snapshots are stored compressed as
//! `<snapshot_dir>/<height>_<datetime>.car.zst`
//! E.g `<snapshot_dir>/64050_2022_11_24T00_00_00Z.car.zst`
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
use std::{path::PathBuf, str::FromStr};

use anyhow::{anyhow, bail, Context as _};
use canonical_path::CanonicalPath;
use itertools::Itertools;
use tempfile::NamedTempFile;
use url::Url;

/// List all snapshots in `snapshot_dir`. Will return `Err(_)` if `snapshot_dir`
/// does not exist.
///
/// Note this function makes blocking syscalls, and should not be called
/// from an async context. Use [tokio::task::spawn_blocking] if needed.
pub fn list(snapshot_dir: &CanonicalPath) -> anyhow::Result<Vec<Snapshot>> {
    walkdir::WalkDir::new(snapshot_dir)
        .sort_by_file_name() // deterministic
        .into_iter()
        .filter_ok(|entry| entry.file_type().is_file())
        .map_ok(|entry| {
            let path = entry.path().to_path_buf();
            let metadata = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse().ok())
                .context("invalid filename for snapshot in snapshot directory")?;
            anyhow::Ok(Snapshot { metadata, path })
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
pub fn clean(snapshot_dir: &CanonicalPath) -> anyhow::Result<()> {
    std::fs::remove_dir_all(snapshot_dir).context("error removing snapshot directory")?;
    std::fs::create_dir_all(snapshot_dir).context("error recreating snapshot dir")?;
    Ok(())
}

/// Fetch a snapshot to snapshot dir
pub async fn fetch(
    snapshot_dir: &CanonicalPath,
    client: &reqwest::Client,
    stable_url: Url,
    progress_bar: &indicatif::ProgressBar,
) -> anyhow::Result<PathBuf> {
    tokio::fs::create_dir_all(snapshot_dir.as_path()).await?;
    let (_meta, file_url, _file_name, file_len) = peek_snapshot(client, stable_url).await?;
    progress_bar.set_length(file_len);
    match download_to_temp(client, file_url, progress_bar).await {
        Ok((path, final_url)) => match metadata_and_filename(&final_url) {
            Ok((_meta, file_name)) => {
                let final_path = snapshot_dir.as_path().join(file_name);
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

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub metadata: SnapshotMetadata,
    /// Full path to snapshot
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use canonical_path::CanonicalPathBuf;
    use httptest::{matchers::request::method_path, responders::status_code, Expectation};
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    /// A useful filename
    const EXAMPLE_FILENAME: &str = "64050_2022_11_24T00_00_00Z.car.zst";

    /// Metadata corresponding to [EXAMPLE_FILENAME]
    fn example_metadata() -> SnapshotMetadata {
        SnapshotMetadata {
            height: 64050,
            datetime: "2022-11-24T00:00:00".parse().unwrap(),
        }
    }

    impl Snapshot {
        fn example(path: PathBuf) -> Self {
            Self {
                metadata: example_metadata(),
                path,
            }
        }
    }

    #[test]
    fn parse_filename() {
        let metadata = SnapshotMetadata::from_str(EXAMPLE_FILENAME).unwrap();
        assert_eq!(example_metadata(), metadata,)
    }

    #[test]
    fn test_list() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?; // name so not dropped

        let snapshot_dir = CanonicalPathBuf::new(temp_dir.path())?
            .file(EXAMPLE_FILENAME)
            .folder_contents("subdir", |slug| {
                slug.file(EXAMPLE_FILENAME)
                    .folder_contents("nested_subdir", |nested_slug| {
                        nested_slug.file(EXAMPLE_FILENAME);
                    });
            });

        let snapshots = list(snapshot_dir.as_canonical_path())?;
        assert_eq!(
            vec![
                Snapshot::example(temp_dir.path().join(EXAMPLE_FILENAME)),
                Snapshot::example(temp_dir.path().join("subdir").join(EXAMPLE_FILENAME)),
                Snapshot::example(
                    temp_dir
                        .path()
                        .join("subdir/nested_subdir")
                        .join(EXAMPLE_FILENAME)
                )
            ],
            snapshots
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let snapshot_dir = CanonicalPath::new(temp_dir.path())?;
        let server = httptest::Server::run();
        server.expect(
            Expectation::matching(method_path("GET", "/stable"))
                .respond_with(status_code(301).insert_header("Location", EXAMPLE_FILENAME)),
        );
        server.expect(
            Expectation::matching(method_path("GET", format!("/{EXAMPLE_FILENAME}")))
                .times(1..)
                .respond_with(status_code(200)),
        );
        fetch(
            snapshot_dir,
            &reqwest::Client::new(),
            server.url_str("/stable").parse().unwrap(),
            &indicatif::ProgressBar::hidden(),
        )
        .await?;
        let snapshots = list(snapshot_dir)?;
        assert_eq!(
            vec![Snapshot::example(temp_dir.path().join(EXAMPLE_FILENAME))],
            snapshots
        );
        Ok(())
    }

    fn assert_one_normal_component(s: &str) -> &Path {
        if let Ok(std::path::Component::Normal(normal)) = Path::new(s).components().exactly_one() {
            return Path::new(normal);
        } else {
            panic!("{s} is not one normal path component")
        };
    }

    /// Utility trait for building a filesystem structure for testing
    trait FileSystemBuilder: AsRef<Path> + Sized {
        fn file(self, name: &str) -> Self {
            self.file_contents(name, [])
        }
        fn file_contents(self, name: &str, contents: impl AsRef<[u8]>) -> Self {
            std::fs::write(
                self.as_ref().join(assert_one_normal_component(name)),
                contents,
            )
            .unwrap();
            self
        }
        fn folder(self, name: &str) -> Self {
            self.folder_contents(name, |_| {})
        }
        fn folder_contents(self, name: &str, make_contents: impl FnOnce(&Path)) -> Self {
            let new_folder_path = self.as_ref().join(assert_one_normal_component(name));
            std::fs::create_dir(&new_folder_path).unwrap();
            make_contents(&new_folder_path);
            self
        }
    }

    impl<T> FileSystemBuilder for T where T: AsRef<Path> {}
}
