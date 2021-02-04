// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

use beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use clock::ChainEpoch;
use fil_types::NetworkVersion;
use std::{error::Error, sync::Arc};
use blocks::{Block, Tipset};
use cid::Cid;
use ipld_blockstore::BlockStore;
mod drand;

#[cfg(not(any(feature = "interopnet")))]
mod mainnet;
#[cfg(not(any(feature = "interopnet")))]
pub use self::mainnet::*;

#[cfg(feature = "interopnet")]
mod interopnet;
#[cfg(feature = "interopnet")]
pub use self::interopnet::*;

/// Migrates the state at a certain epoch for hardforks that are state backwards incompatible like actors upgrades.
/// (old_state: Cid, epoch: ChainEpoch, ts: Arc<Tipset>)
/// old_state: the state root pre migration
/// epoch: the epoch at which the upgrade happens
/// ts: the last non-null tipset before the upgrade
/// TODO: Definitely some params needed may be missing here... 
type MigrateFn = dyn FnMut(&dyn BlockStore, Cid, ChainEpoch, Arc<Tipset>) -> Cid;

/// Defines the different hard fork parameters.
struct Upgrade
    /// When the hard fork will happen
    height: ChainEpoch,
    /// The version of the fork
    network: NetworkVersion,
    /// Migrates the state tree to a new version.
    /// Necessary when there is no backwards compatability due to new Actor state definitions for example.
    migrate: Option<Box<MigrateFn>>,
}

struct DrandPoint<'a> {
    pub height: ChainEpoch,
    pub config: &'a DrandConfig<'a>,
}

const VERSION_SCHEDULE: [Upgrade; 9] = [
    Upgrade {
        height: UPGRADE_BREEZE_HEIGHT,
        network: NetworkVersion::V1,
        migrate: None,
    },
    Upgrade {
        height: UPGRADE_SMOKE_HEIGHT,
        network: NetworkVersion::V2,
        migrate: None,
    },
    Upgrade {
        height: UPGRADE_IGNITION_HEIGHT,
        network: NetworkVersion::V3,
        migrate: None,
    },
    Upgrade {
        height: UPGRADE_ACTORS_V2_HEIGHT,
        network: NetworkVersion::V4,
        migrate: None,
    },
    Upgrade {
        height: UPGRADE_TAPE_HEIGHT,
        network: NetworkVersion::V5,
        migrate: None,
    },
    Upgrade {
        height: UPGRADE_KUMQUAT_HEIGHT,
        network: NetworkVersion::V6,
        migrate: None,

    },
    Upgrade {
        height: UPGRADE_CALICO_HEIGHT,
        network: NetworkVersion::V7,
        migrate: None,
    },
    Upgrade {
        height: UPGRADE_PERSIAN_HEIGHT,
        network: NetworkVersion::V8,
        migrate: None,
    },
    Upgrade {
        height: UPGRADE_ORANGE_HEIGHT,
        network: NetworkVersion::V9,
        migrate: None,
    },
];

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
