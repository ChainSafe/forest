use anyhow::{anyhow, bail, ensure, Result};
use nom::{
    bytes::complete::tag,
    character::complete::digit1,
    combinator::{map_res, recognize},
    multi::many1,
    sequence::tuple,
};
use std::ops::RangeInclusive;
use std::path::Path;
use std::process::{Stdio, Command, Child};
use std::str::FromStr;

use super::{ChainEpoch, R2_ENDPOINT};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct HistoricalSnapshot {
    pub path: String,
    pub epoch_range: RangeInclusive<ChainEpoch>,
    pub size: u64,
}

impl HistoricalSnapshot {
    // 2023-08-08 15:31:03 26394404409 snapshot_950400_953282_1667057812.car.zst
    fn parse(ls_string: &str) -> Result<HistoricalSnapshot> {
        match ls_string.split_whitespace().collect::<Vec<_>>().as_slice() {
            [_date, _time, bytes, path] => {
                let bytes: u64 = bytes.parse()?;
                let (_, start_epoch, _, end_epoch) = enter_nom(
                    tuple((tag("snapshot_"), number::<u64>, tag("_"), number)),
                    path,
                )?;
                Ok(HistoricalSnapshot {
                    path: path.to_string(),
                    epoch_range: RangeInclusive::new(start_epoch, end_epoch),
                    size: bytes,
                })
            }
            _ => bail!("unexpected historical snapshot string"),
        }
    }

    pub fn new() -> Result<Vec<HistoricalSnapshot>> {
        let output = Command::new("aws")
            .arg("--endpoint")
            .arg(R2_ENDPOINT)
            .arg("s3")
            .arg("ls")
            .arg("s3://forest-archive/historical/")
            .output()?;
        ensure!(
            output.status.success(),
            "failed to list historical snapshots"
        );
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
        let status = Command::new("wget")
            .arg(format!(
                "https://forest-archive.chainsafe.dev/historical/{}",
                self.path
            ))
            .arg("--output-document")
            .arg(dst)
            .status()?;
        anyhow::ensure!(status.success());
        Ok(())
    }

    // Download and encode
    pub fn encode(&self) -> Result<Child> {
        let mut curl = Command::new("curl")
            .arg(format!(
                "https://forest-archive.chainsafe.dev/historical/{}",
                self.path
            ))
            .arg("--silent")
            .stdout(Stdio::piped())
            .spawn()?;
        Ok(Command::new("forest-cli")
            .arg("snapshot")
            .arg("compress")
            .arg("-")
            .arg("--output")
            .arg(&self.path)
            .stdin(curl.stdout.take().unwrap())
            .spawn()?)
    }

    pub fn path(&self) -> &str {
        &self.path
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
