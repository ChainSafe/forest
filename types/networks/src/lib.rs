// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

use beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use clock::ChainEpoch;
use fil_types::NetworkVersion;
use std::{error::Error, sync::Arc};
mod drand;

#[cfg(feature = "mainnet")]
mod mainnet;
#[cfg(feature = "mainnet")]
pub use self::mainnet::*;

#[cfg(all(feature = "interopnet", not(feature = "devnet")))]
mod interopnet;
#[cfg(all(feature = "interopnet", not(feature = "devnet")))]
pub use self::interopnet::*;

#[cfg(all(feature = "devnet", not(feature = "interopnet")))]
mod devnet;
#[cfg(all(feature = "devnet", not(feature = "interopnet")))]
pub use self::devnet::*;

/// Defines the different hard fork parameters.
struct Upgrade {
    /// When the hard fork will happen
    height: ChainEpoch,
    /// The version of the fork
    network: NetworkVersion,
}

struct DrandPoint<'a> {
    pub height: ChainEpoch,
    pub config: &'a DrandConfig<'a>,
}

const VERSION_SCHEDULE: [Upgrade; 12] = [
    Upgrade {
        height: UPGRADE_BREEZE_HEIGHT,
        network: NetworkVersion::V1,
    },
    Upgrade {
        height: UPGRADE_SMOKE_HEIGHT,
        network: NetworkVersion::V2,
    },
    Upgrade {
        height: UPGRADE_IGNITION_HEIGHT,
        network: NetworkVersion::V3,
    },
    Upgrade {
        height: UPGRADE_ACTORS_V2_HEIGHT,
        network: NetworkVersion::V4,
    },
    Upgrade {
        height: UPGRADE_TAPE_HEIGHT,
        network: NetworkVersion::V5,
    },
    Upgrade {
        height: UPGRADE_KUMQUAT_HEIGHT,
        network: NetworkVersion::V6,
    },
    Upgrade {
        height: UPGRADE_CALICO_HEIGHT,
        network: NetworkVersion::V7,
    },
    Upgrade {
        height: UPGRADE_PERSIAN_HEIGHT,
        network: NetworkVersion::V8,
    },
    Upgrade {
        height: UPGRADE_ORANGE_HEIGHT,
        network: NetworkVersion::V9,
    },
    Upgrade {
        height: UPGRADE_ACTORS_V3_HEIGHT,
        network: NetworkVersion::V10,
    },
    Upgrade {
        height: UPGRADE_NORWEGIAN_HEIGHT,
        network: NetworkVersion::V11,
    },
    Upgrade {
        height: UPGRADE_ACTORS_V4_HEIGHT,
        network: NetworkVersion::V12,
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
