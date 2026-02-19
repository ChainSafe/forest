// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fmt::Display,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{cli_shared::snapshot::parse::ParsedFilename, utils::net::download_file_with_retry};
use crate::{networks::NetworkChain, utils::net::DownloadFileOption};
use anyhow::{Context as _, bail};
use chrono::NaiveDate;
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
pub enum TrustedVendor {
    #[default]
    Forest,
}

/// Create a filename in the "full" format. See [`parse`].
// Common between export, and [`fetch`].
// Keep in sync with the CLI documentation for the `snapshot` sub-command.
pub fn filename(
    vendor: impl Display,
    chain: impl Display,
    date: NaiveDate,
    height: i64,
    forest_format: bool,
) -> String {
    let vendor = vendor.to_string();
    let chain = chain.to_string();
    ParsedFilename::Full {
        vendor: &vendor,
        chain: &chain,
        date,
        height,
        forest_format,
    }
    .to_string()
}

/// Returns the path to the downloaded file.
pub async fn fetch(
    directory: &Path,
    chain: &NetworkChain,
    vendor: TrustedVendor,
) -> anyhow::Result<PathBuf> {
    let (url, _len, path) = peek(vendor, chain).await?;
    let (date, height, forest_format) = ParsedFilename::parse_str(&path)
        .context("unexpected path format")?
        .date_and_height_and_forest();
    let filename = filename(vendor, chain, date, height, forest_format);

    tracing::info!("Downloading snapshot: {filename}");

    download_file_with_retry(
        &url,
        directory,
        &filename,
        DownloadFileOption::Resumable,
        None,
    )
    .await
}

/// Returns
/// - The final URL after redirection(s)
/// - The size of the snapshot from this vendor on this chain
/// - The filename of the snapshot
pub async fn peek(
    vendor: TrustedVendor,
    chain: &NetworkChain,
) -> anyhow::Result<(Url, u64, String)> {
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
    let final_url = response.url().clone();
    let cd_path = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(parse_content_disposition);
    Ok((
        final_url,
        response
            .content_length()
            .context("no content-length header")?,
        cd_path.context("no content-disposition filepath")?,
    ))
}

// Extract file paths from content-disposition values:
//   "attachment; filename=\"911520_2023_09_14T06_13_00Z.car.zst\""
// => "911520_2023_09_14T06_13_00Z.car.zst"
fn parse_content_disposition(value: &reqwest::header::HeaderValue) -> Option<String> {
    use regex::Regex;
    let re = Regex::new("filename=\"([^\"]+)\"").ok()?;
    let cap = re.captures(value.to_str().ok()?)?;
    Some(cap.get(1)?.as_str().to_owned())
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
    const FOREST_MAINNET_COMPRESSED: &str = "https://forest-archive.chainsafe.dev/latest/mainnet/";
    const FOREST_CALIBNET_COMPRESSED: &str =
        "https://forest-archive.chainsafe.dev/latest/calibnet/";
);

pub fn stable_url(vendor: TrustedVendor, chain: &NetworkChain) -> anyhow::Result<Url> {
    let s = match (vendor, chain) {
        (TrustedVendor::Forest, NetworkChain::Mainnet) => FOREST_MAINNET_COMPRESSED,
        (TrustedVendor::Forest, NetworkChain::Calibnet) => FOREST_CALIBNET_COMPRESSED,
        (TrustedVendor::Forest, NetworkChain::Butterflynet | NetworkChain::Devnet(_)) => {
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
    //! Vendors publish filenames with two formats:
    //! `filecoin_snapshot_calibnet_2023-06-13_height_643680.car.zst` "full" and
    //! `632400_2023_06_09T08_13_00Z.car.zst` "short".
    //!
    //! This module contains utilities for parsing and printing these formats.

    use std::{fmt::Display, str::FromStr};

    use anyhow::{anyhow, bail};
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
    use nom::{
        Err, Parser,
        branch::alt,
        bytes::complete::{tag, take_until},
        character::complete::digit1,
        combinator::{map_res, recognize},
        error::ErrorKind,
        error_position,
        multi::many1,
    };

    use crate::db::car::forest::FOREST_CAR_FILE_EXTENSION;

    #[derive(PartialEq, Debug, Clone, Hash)]
    pub(super) enum ParsedFilename<'a> {
        Short {
            date: NaiveDate,
            time: NaiveTime,
            height: i64,
        },
        Full {
            vendor: &'a str,
            chain: &'a str,
            date: NaiveDate,
            height: i64,
            forest_format: bool,
        },
    }

    impl Display for ParsedFilename<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                ParsedFilename::Short { date, time, height } => f.write_fmt(format_args!(
                    "{height}_{}.car.zst",
                    NaiveDateTime::new(*date, *time).format("%Y_%m_%dT%H_%M_%SZ")
                )),
                ParsedFilename::Full {
                    vendor,
                    chain,
                    date,
                    height,
                    forest_format,
                } => f.write_fmt(format_args!(
                    "{vendor}_snapshot_{chain}_{}_height_{height}{}.car.zst",
                    date.format("%Y-%m-%d"),
                    if *forest_format { ".forest" } else { "" }
                )),
            }
        }
    }

    impl<'a> ParsedFilename<'a> {
        pub fn date_and_height_and_forest(&self) -> (NaiveDate, i64, bool) {
            match self {
                ParsedFilename::Short { date, height, .. } => (*date, *height, false),
                ParsedFilename::Full {
                    date,
                    height,
                    forest_format,
                    ..
                } => (*date, *height, *forest_format),
            }
        }

        pub fn parse_str(input: &'a str) -> anyhow::Result<Self> {
            enter_nom(alt((short, full)), input)
        }
    }

    /// Parse a number using its [`FromStr`] implementation.
    fn number<T>(input: &str) -> nom::IResult<&str, T>
    where
        T: FromStr,
    {
        map_res(recognize(many1(digit1)), T::from_str).parse(input)
    }

    /// Create a parser for `YYYY-MM-DD` etc
    fn ymd(separator: &str) -> impl Fn(&str) -> nom::IResult<&str, NaiveDate> + '_ {
        move |input| {
            let (rest, (year, _, month, _, day)) =
                (number, tag(separator), number, tag(separator), number).parse(input)?;
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
                (number, tag(separator), number, tag(separator), number).parse(input)?;
            match NaiveTime::from_hms_opt(hour, minute, second) {
                Some(date) => Ok((rest, date)),
                None => Err(Err::Error(error_position!(input, ErrorKind::Verify))),
            }
        }
    }

    fn full(input: &str) -> nom::IResult<&str, ParsedFilename<'_>> {
        let (rest, (vendor, _snapshot_, chain, _, date, _height_, height, car_zst)) = (
            take_until("_snapshot_"),
            tag("_snapshot_"),
            take_until("_"),
            tag("_"),
            ymd("-"),
            tag("_height_"),
            number,
            alt((tag(".car.zst"), tag(FOREST_CAR_FILE_EXTENSION))),
        )
            .parse(input)?;
        Ok((
            rest,
            ParsedFilename::Full {
                vendor,
                chain,
                date,
                height,
                forest_format: car_zst == FOREST_CAR_FILE_EXTENSION,
            },
        ))
    }

    fn short(input: &str) -> nom::IResult<&str, ParsedFilename<'_>> {
        let (rest, (height, _, date, _, time, _)) = (
            number,
            tag("_"),
            ymd("_"),
            tag("T"),
            hms("_"),
            tag("Z.car.zst"),
        )
            .parse(input)?;
        Ok((rest, ParsedFilename::Short { date, time, height }))
    }

    fn enter_nom<'a, T>(
        mut parser: impl nom::Parser<&'a str, Output = T, Error = nom::error::Error<&'a str>>,
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_serialization() {
            for (text, value) in [
                (
                    "forest_snapshot_mainnet_2023-05-30_height_2905376.car.zst",
                    ParsedFilename::full("forest", "mainnet", 2023, 5, 30, 2905376, false),
                ),
                (
                    "forest_snapshot_calibnet_2023-05-30_height_604419.car.zst",
                    ParsedFilename::full("forest", "calibnet", 2023, 5, 30, 604419, false),
                ),
                (
                    "forest_snapshot_mainnet_2023-05-30_height_2905376.forest.car.zst",
                    ParsedFilename::full("forest", "mainnet", 2023, 5, 30, 2905376, true),
                ),
                (
                    "forest_snapshot_calibnet_2023-05-30_height_604419.forest.car.zst",
                    ParsedFilename::full("forest", "calibnet", 2023, 5, 30, 604419, true),
                ),
                (
                    "2905920_2023_05_30T22_00_00Z.car.zst",
                    ParsedFilename::short(2905920, 2023, 5, 30, 22, 0, 0),
                ),
                (
                    "605520_2023_05_31T00_13_00Z.car.zst",
                    ParsedFilename::short(605520, 2023, 5, 31, 0, 13, 0),
                ),
                (
                    "filecoin_snapshot_calibnet_2023-06-13_height_643680.car.zst",
                    ParsedFilename::full("filecoin", "calibnet", 2023, 6, 13, 643680, false),
                ),
                (
                    "venus_snapshot_pineconenet_2045-01-01_height_2.car.zst",
                    ParsedFilename::full("venus", "pineconenet", 2045, 1, 1, 2, false),
                ),
                (
                    "filecoin_snapshot_calibnet_2023-06-13_height_643680.forest.car.zst",
                    ParsedFilename::full("filecoin", "calibnet", 2023, 6, 13, 643680, true),
                ),
                (
                    "venus_snapshot_pineconenet_2045-01-01_height_2.forest.car.zst",
                    ParsedFilename::full("venus", "pineconenet", 2045, 1, 1, 2, true),
                ),
            ] {
                assert_eq!(
                    value,
                    ParsedFilename::parse_str(text).unwrap(),
                    "mismatch in deserialize"
                );
                assert_eq!(value.to_string(), text, "mismatch in serialize");
            }
        }

        #[test]
        fn test_wrong_ext() {
            ParsedFilename::parse_str("forest_snapshot_mainnet_2023-05-30_height_2905376.car.zstt")
                .unwrap_err();
            ParsedFilename::parse_str(
                "forest_snapshot_mainnet_2023-05-30_height_2905376.car.zst.tmp",
            )
            .unwrap_err();
        }

        impl ParsedFilename<'static> {
            /// # Panics
            /// - If `ymd`/`hms` aren't valid
            fn short(
                height: i64,
                year: i32,
                month: u32,
                day: u32,
                hour: u32,
                min: u32,
                sec: u32,
            ) -> Self {
                Self::Short {
                    date: NaiveDate::from_ymd_opt(year, month, day).unwrap(),
                    time: NaiveTime::from_hms_opt(hour, min, sec).unwrap(),
                    height,
                }
            }
        }

        impl<'a> ParsedFilename<'a> {
            /// # Panics
            /// - If `ymd` isn't valid
            fn full(
                vendor: &'a str,
                chain: &'a str,
                year: i32,
                month: u32,
                day: u32,
                height: i64,
                forest_format: bool,
            ) -> Self {
                Self::Full {
                    vendor,
                    chain,
                    date: NaiveDate::from_ymd_opt(year, month, day).unwrap(),
                    height,
                    forest_format,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_content_disposition;
    use reqwest::header::HeaderValue;

    #[test]
    fn content_disposition_forest() {
        assert_eq!(
            parse_content_disposition(&HeaderValue::from_static(
                "attachment; filename*=UTF-8''forest_snapshot_calibnet_2023-09-14_height_911888.forest.car.zst; \
                 filename=\"forest_snapshot_calibnet_2023-09-14_height_911888.forest.car.zst\""
            )).unwrap(),
            "forest_snapshot_calibnet_2023-09-14_height_911888.forest.car.zst"
        );
    }
}
