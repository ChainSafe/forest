use anyhow::{anyhow, bail, Context, Result, ensure};
use nom::{
    bytes::complete::tag,
    character::complete::digit1,
    combinator::{map_res, recognize},
    multi::many1,
    sequence::tuple,
};
use std::{ops::RangeInclusive};
use std::process::Command;
use std::str::FromStr;
use url::Url;
use which::which;

mod historical;
use historical::HistoricalSnapshot;

const FOREST_PROJECT: &str = "forest-391213";

type ChainEpoch = u64;

fn main() -> Result<()> {
    which("forest").context("Failed to find the 'forest' binary.\nSee installation instructions: https://github.com/ChainSafe/forest")?;
    which("gsutil").context("Failed to find the 'gsutil' binary.\nSee installation instructions: https://cloud.google.com/storage/docs/gsutil_install")?;

    let snapshots = HistoricalSnapshot::new()?;
    for snapshot in snapshots {
        println!("{:?}", snapshot);
    }

    Ok(())
}

