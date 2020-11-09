// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod beacon_entries;
mod drand;
mod mock_beacon;

pub use beacon_entries::*;
pub use drand::*;
pub use mock_beacon::*;

use clock::ChainEpoch;
use std::error::Error;

pub async fn beacon_entries_for_block<B: Beacon>(
    beacon: &B,
    round: ChainEpoch,
    prev: &BeaconEntry,
) -> Result<Vec<BeaconEntry>, Box<dyn Error>> {
    let max_round = beacon.max_beacon_round_for_epoch(round);
    if max_round == prev.round() {
        return Ok(vec![]);
    }
    // TODO: this is a sketchy way to handle the genesis block not having a beacon entry
    let prev_round = if prev.round() == 0 {
        max_round - 1
    } else {
        prev.round()
    };

    let mut cur = max_round;
    let mut out = Vec::new();
    while cur > prev_round {
        let entry = beacon.entry(cur).await?;
        cur = entry.round() - 1;
        out.push(entry);
    }
    out.reverse();
    Ok(out)
}
