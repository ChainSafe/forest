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

    // forest_snapshot_calibnet_2023-05-12_height_30000.forest.car.zst
    // forest_diff_calibnet_2023-05-12_height_30000+3000.forest.car.zst
    // Snapshot at epoch 0
    //  1 diff 00000+3000
    //  2 diff 03000+3000
    //  3 diff 06000+3000
    //  4 diff 09000+3000
    //  5 diff 12000+3000
    //  6 diff 15000+3000
    //  7 diff 18000+3000
    //  8 diff 21000+3000
    //  9 diff 24000+3000
    // 10 diff 27000+3000
    // Snapshot at epoch 30000
    // roughly 2880 in a day.
    // 00000-29999
    // 30000-59999

    Ok(())
}

