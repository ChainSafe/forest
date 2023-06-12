// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fmt::Display,
    io,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context as _};
use chrono::NaiveDate;
use forest_networks::NetworkChain;
use forest_utils::io::progress_bar::downloading_style;
use tracing::{info, warn};
use url::Url;

/// Who hosts the snapshot on the web?
/// See [`stable_url`].
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    PartialEq,
    Eq,
    Default,
    strum::EnumString, // impl std::str::FromStr
    strum::Display,    // impl Display
    clap::ValueEnum,   // allow values to be enumerated and parsed by clap
)]
#[strum(serialize_all = "kebab-case")]
pub enum Vendor {
    #[default]
    Forest,
    Filops,
}

/// Common format for filenames for export and [`fetch`].
/// Keep in sync with the CLI documentation for the `export` sub-command.
pub fn filename(chain: impl Display, date: NaiveDate, height: i64) -> String {
    format!(
        "forest_snapshot_{chain}_date_{}_height_{height}.car.zst",
        date.format("%Y-%m-%d")
    )
}

/// Fetch a compressed snapshot with `aria2c`, falling back to our own HTTP client.
/// Returns the path to the downloaded file, which matches the format in .
pub async fn fetch(
    directory: &Path,
    chain: &NetworkChain,
    vendor: Vendor,
) -> anyhow::Result<PathBuf> {
    let (_len, url) = peek(vendor, chain).await?;
    let (height, date, _time) = parse::parse_url(&url)?;
    let filename = filename(chain, date, height);

    match download_aria2c(&url, directory, &filename).await {
        Ok(path) => Ok(path),
        Err(AriaErr::CouldNotExec(reason)) => {
            warn!(%reason, "couldn't run aria2c. Falling back to conventional download, which will be much slower - consider installing aria2c.");
            download_http(url, directory, &filename).await
        }
        Err(AriaErr::Other(o)) => Err(o),
    }
}

/// Returns
/// - The size of the snapshot from this vendor on this chain
/// - The final URL of the snapshot
pub async fn peek(vendor: Vendor, chain: &NetworkChain) -> anyhow::Result<(u64, Url)> {
    let stable_url = stable_url(vendor, chain)?;
    // issue an actual GET, so the content length will be of the body
    // (we never actually fetch the body)
    // if we issue a HEAD, the content-length will be zero for our stable URLs
    // (this is a bug, maybe in reqwest - HEAD _should_ give us the length)
    // (probably because the stable URLs are all double-redirects 301 -> 302 -> 200)
    let response = reqwest::get(stable_url)
        .await?
        .error_for_status()
        .context("server returned an error response")?;

    Ok((
        response
            .content_length()
            .context("no content-length header")?,
        response.url().clone(),
    ))
}

enum AriaErr {
    CouldNotExec(io::Error),
    Other(anyhow::Error),
}

/// Run `aria2c`, with inherited stdout and stderr (so output will be printed).
async fn download_aria2c(url: &Url, directory: &Path, filename: &str) -> Result<PathBuf, AriaErr> {
    let exit_status = tokio::process::Command::new("aria2c")
        .args([
            "--continue=true",
            "--max-tries=0",
            // Download chunks concurrently, resulting in dramatically faster downloads
            "--split=5",
            "--max-connection-per-server=5",
            format!("--out={filename}").as_str(),
            "--dir",
        ])
        .arg(directory)
        .arg(url.as_str())
        .kill_on_drop(true) // allow cancellation
        .spawn() // defaults to inherited stdio
        .map_err(AriaErr::CouldNotExec)?
        .wait()
        .await
        .map_err(|it| AriaErr::Other(it.into()))?;

    match exit_status.success() {
        true => Ok(directory.join(filename)),
        false => {
            let msg = exit_status
                .code()
                .map(|it| it.to_string())
                .unwrap_or_else(|| String::from("<killed>"));
            Err(AriaErr::Other(anyhow!("running aria2c failed: {msg}")))
        }
    }
}

/// Download the file at `url` with a private HTTP client, returning the path to the downloaded file
async fn download_http(url: Url, directory: &Path, filename: &str) -> anyhow::Result<PathBuf> {
    use futures::TryStreamExt as _;
    use tap::Pipe as _;
    let dst_path = directory.join(filename);
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
    let mut dst = tokio::fs::File::create(&dst_path)
        .await
        .context("couldn't create destination file")?;
    tokio::io::copy(&mut src, &mut dst)
        .await
        .map(|_| dst_path)
        .context("couldn't download file")
}

/// Also defines an `ALL_URLS` constant for test purposes
macro_rules! define_urls {
    ($($vis:vis const $name:ident: &str = $value:literal;)* $(,)?) => {
        $($vis const $name: &str = $value;)*

        #[cfg(test)]
        const ALL_URLS: &[&str] = [
            $($name,)*
        ].as_slice();
    };
}

define_urls!(
    const FOREST_MAINNET_COMPRESSED: &str =
        "https://forest.chainsafe.io/mainnet/snapshot-latest.car.zst";
    const FOREST_CALIBNET_COMPRESSED: &str =
        "https://forest.chainsafe.io/calibnet/snapshot-latest.car.zst";
    const FILOPS_MAINNET_COMPRESSED: &str =
        "https://snapshots.mainnet.filops.net/minimal/latest.zst";
    const FILOPS_CALIBNET_COMPRESSED: &str =
        "https://snapshots.calibrationnet.filops.net/minimal/latest.zst";
);

fn stable_url(vendor: Vendor, chain: &NetworkChain) -> anyhow::Result<Url> {
    let s = match (vendor, chain) {
        (Vendor::Forest, NetworkChain::Mainnet) => FOREST_MAINNET_COMPRESSED,
        (Vendor::Forest, NetworkChain::Calibnet) => FOREST_CALIBNET_COMPRESSED,
        (Vendor::Filops, NetworkChain::Mainnet) => FILOPS_MAINNET_COMPRESSED,
        (Vendor::Filops, NetworkChain::Calibnet) => FILOPS_CALIBNET_COMPRESSED,
        (Vendor::Forest | Vendor::Filops, NetworkChain::Devnet(_)) => {
            bail!("unsupported chain {chain}")
        }
    };
    Ok(Url::from_str(s).unwrap())
}

#[test]
fn parse_stable_urls() {
    for url in ALL_URLS {
        let _did_not_panic = Url::from_str(url).unwrap();
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
