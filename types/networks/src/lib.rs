// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

use beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use cid::Cid;
use clock::ChainEpoch;
use fil_types::NetworkVersion;
use ipld_blockstore::BlockStore;
use std::{error::Error, sync::Arc};
mod drand;

#[cfg(not(any(feature = "interopnet")))]
mod mainnet;
#[cfg(not(any(feature = "interopnet")))]
pub use self::mainnet::*;

#[cfg(feature = "interopnet")]
mod interopnet;
#[cfg(feature = "interopnet")]
pub use self::interopnet::*;

/// Defines the different hard fork parameters.
struct Upgrade {
    /// When the hard fork will happen
    height: ChainEpoch,
    /// The version of the fork
    network: NetworkVersion,
    /// Is there a migration needed for this upgrade?
    /// Note: We do not support Pre Version 9, so even if there is
    /// an upgrade there, we don't do a migration.
    migration: bool,
}

struct DrandPoint<'a> {
    pub height: ChainEpoch,
    pub config: &'a DrandConfig<'a>,
}

const VERSION_SCHEDULE: [Upgrade; 9] = [
    Upgrade {
        height: UPGRADE_BREEZE_HEIGHT,
        network: NetworkVersion::V1,
        migration: false,
    },
    Upgrade {
        height: UPGRADE_SMOKE_HEIGHT,
        network: NetworkVersion::V2,
        migration: false,
    },
    Upgrade {
        height: UPGRADE_IGNITION_HEIGHT,
        network: NetworkVersion::V3,
        migration: false,
    },
    Upgrade {
        height: UPGRADE_ACTORS_V2_HEIGHT,
        network: NetworkVersion::V4,
        migration: false,
    },
    Upgrade {
        height: UPGRADE_TAPE_HEIGHT,
        network: NetworkVersion::V5,
        migration: false,
    },
    Upgrade {
        height: UPGRADE_KUMQUAT_HEIGHT,
        network: NetworkVersion::V6,
        migration: false,
    },
    Upgrade {
        height: UPGRADE_CALICO_HEIGHT,
        network: NetworkVersion::V7,
        migration: false,
    },
    Upgrade {
        height: UPGRADE_PERSIAN_HEIGHT,
        network: NetworkVersion::V8,
        migration: false,
    },
    Upgrade {
        height: UPGRADE_ORANGE_HEIGHT,
        network: NetworkVersion::V9,
        migration: false,
    },
];

/// Gets network version from epoch using default Mainnet schedule.
pub fn get_network_version_default(epoch: ChainEpoch) -> NetworkVersion {
    VERSION_SCHEDULE
        .iter()
        .rev()
        .find(|upgrade| epoch > upgrade.height)
        .map(|upgrade| upgrade.network)
        .unwrap_or(NetworkVersion::V0)
}

/// If true, then we should perform a state migration at this epoch, otherwise, no migration.
pub fn is_migrate_epoch(epoch: ChainEpoch) -> bool {
    VERSION_SCHEDULE
        .iter()
        .rev()
        .find(|upgrade| epoch == upgrade.height)
        .map(|upgrade| upgrade.migration)
        .unwrap_or(false)
}

/// Calls the state migration functions to migrate to the new StateTree.
/// Currently this is a TODO because we havent implemented and migrations yet.
/// The first migration to implement is from V2 to V3 (Network Version 10), so this method
/// signature will probably change as we discover more things we need to pass in + error handling.
pub fn migrate_state<T>(_bs: &T, _old_state: Cid, _epoch: ChainEpoch) -> Cid
where
    T: BlockStore,
{
    todo!()
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
