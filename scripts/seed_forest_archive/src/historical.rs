use anyhow::{anyhow, bail, ensure, Context, Result};
use nom::{
    bytes::complete::tag,
    character::complete::digit1,
    combinator::{map_res, recognize},
    multi::many1,
    sequence::tuple,
};
use std::ops::RangeInclusive;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use url::Url;

use super::ChainEpoch;
use super::FOREST_PROJECT;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct HistoricalSnapshot {
    pub url: Url,
    pub epoch_range: RangeInclusive<ChainEpoch>,
    pub size: u64,
}

impl HistoricalSnapshot {
    // Parse strings such as '25952343569  gs://fil-mainnet-archival-snapshots/historical-exports/snapshot_930240_933122_1667057892.car.zst'
    fn parse(du_string: &str) -> Result<HistoricalSnapshot> {
        match du_string.split_whitespace().collect::<Vec<_>>().as_slice() {
            [bytes, url] => {
                let bytes: u64 = bytes.parse()?;
                let url = Url::parse(url)?;
                let last_segment = url
                    .path_segments()
                    .context("unexpected base url")?
                    .last()
                    .context("unexpected url with no filename")?;
                let (_, start_epoch, _, end_epoch) = enter_nom(
                    tuple((tag("snapshot_"), number::<u64>, tag("_"), number)),
                    last_segment,
                )?;
                Ok(HistoricalSnapshot {
                    url,
                    epoch_range: RangeInclusive::new(start_epoch, end_epoch),
                    size: bytes,
                })
            }
            _ => bail!("unexpected historical snapshot string"),
        }
    }

    pub fn new() -> Result<Vec<HistoricalSnapshot>> {
        let output = Command::new("gsutil")
            .arg("-u")
            .arg(FOREST_PROJECT)
            .arg("du")
            .arg("gs://fil-mainnet-archival-snapshots/historical-exports/*")
            .output()?;
        ensure!(output.status.success());
        ensure!(output.stderr.is_empty());
        std::str::from_utf8(&output.stdout)?
            .lines()
            .map(HistoricalSnapshot::parse)
            .collect::<Result<Vec<_>>>()
    }

    pub fn highest_epoch(&self) -> ChainEpoch {
        *self.epoch_range.end()
    }

    pub fn download(&self, dst: &Path) -> Result<()> {
        let status = Command::new("gsutil")
            .arg("-u")
            .arg(FOREST_PROJECT)
            .arg("cp")
            .arg(self.url.to_string())
            .arg(dst)
            .status()?;
        anyhow::ensure!(status.success());
        Ok(())
    }
}

fn number<T>(input: &str) -> nom::IResult<&str, T>
where
    T: FromStr,
{
    map_res(recognize(many1(digit1)), T::from_str)(input)
}

fn enter_nom<'a, T>(
    mut parser: impl nom::Parser<&'a str, T, nom::error::Error<&'a str>>,
    input: &'a str,
) -> anyhow::Result<T> {
    let (_rest, t) = parser
        .parse(input)
        .map_err(|e| anyhow!("Parser error: {e}"))?;
    Ok(t)
}
