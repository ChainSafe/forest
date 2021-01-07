// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

use beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use clock::ChainEpoch;
use fil_types::{NetworkVersion, BLOCK_DELAY_SECS};
use std::{error::Error, sync::Arc};

mod drand;

mod mainnet;
pub use self::mainnet::*;

struct Upgrade {
    height: ChainEpoch,
    network: NetworkVersion,
}

struct DrandPoint<'a> {
    pub height: ChainEpoch,
    pub config: &'a DrandConfig<'a>,
}

/// Gets network version from epoch using default Mainnet schedule
pub fn get_network_version_default(epoch: ChainEpoch) -> NetworkVersion {
    VERSION_SCHEDULE
        .iter()
        .rev()
        .find(|upgrade| epoch > upgrade.height)
        .map(|upgrade| upgrade.network)
        .unwrap_or(NetworkVersion::V0)
}

/// Constructs a drand beacon schedule based on the build config.
pub async fn beacon_schedule_default(
    genesis_ts: u64,
) -> Result<BeaconSchedule<DrandBeacon>, Box<dyn Error>> {
    let mut points = BeaconSchedule(Vec::with_capacity(DRAND_SCHEDULE.len()));
    for dc in DRAND_SCHEDULE.iter() {
        points.0.push(BeaconPoint {
            height: dc.height,
            beacon: Arc::new(DrandBeacon::new(genesis_ts, BLOCK_DELAY_SECS, dc.config).await?),
        });
    }
    Ok(points)
}
