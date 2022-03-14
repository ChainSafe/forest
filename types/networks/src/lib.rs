// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

use beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use clock::ChainEpoch;
use fil_types::NetworkVersion;
use std::{error::Error, sync::Arc};

mod drand;
mod mainnet;

/// Newest network version for all networks
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V14;

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

/// Defines the meaningful heights of the protocol.
pub enum Height {
    Breeze,
    Smoke,
    Ignition,
    ActorsV2,
    Tape,
    Liftoff,
    Kumquat,
    Calico,
    Persian,
    Orange,
    Claus,
    Trust,
    Norwegian,
    Turbo,
    Hyperdrive,
    Chocolate,
    OhSnap,
}

const MAINNET_VERSION_SCHEDULE: [Upgrade; 14] = {
    use self::mainnet::*;
    [
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
        Upgrade {
            height: UPGRADE_HYPERDRIVE_HEIGHT,
            network: NetworkVersion::V13,
        },
        Upgrade {
            height: UPGRADE_ACTORS_V6_HEIGHT,
            network: NetworkVersion::V14,
        }
    ]
};

/// Config used when initializing a network.
pub struct Config<'a> {
    name: String,
    version_schedule: [Upgrade; 14],
    drand_schedule: Vec<DrandPoint<'a>>,
    genesis_bytes: Vec<u8>,
    bootstrap_peers: Vec<String>,
    block_delay_secs: u64,
}

impl<'a> Config<'a> {
    pub fn mainnet() -> Self {
        Self {
            name: "mainnet".to_string(),
            version_schedule: MAINNET_VERSION_SCHEDULE,
            drand_schedule: vec!(),
            genesis_bytes: vec!(),
            bootstrap_peers: vec!(),
            block_delay_secs: mainnet::BLOCK_DELAY_SECS,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        self.version_schedule
            .iter()
            .rev()
            .find(|upgrade| epoch > upgrade.height)
            .map(|upgrade| upgrade.network)
            .unwrap_or(NetworkVersion::V0)
    }

    pub async fn get_beacon_schedule(
        &self,
        genesis_ts: u64,
    ) -> Result<BeaconSchedule<DrandBeacon>, Box<dyn Error>> {
        let mut points = BeaconSchedule(Vec::with_capacity(self.drand_schedule.len()));
        for dc in self.drand_schedule.iter() {
            points.0.push(BeaconPoint {
                height: dc.height,
                beacon: Arc::new(DrandBeacon::new(genesis_ts, self.block_delay(), dc.config).await?),
            });
        }
        Ok(points)
    }

    pub fn genesis_bytes(&self) -> &[u8] {
        &self.genesis_bytes
    }

    pub fn bootstrap_peers(&self) -> &[String] {
        &self.bootstrap_peers
    }

    pub fn block_delay(&self) -> u64 {
        self.block_delay_secs
    }

    pub fn epoch(&self, height: Height) -> ChainEpoch {
        todo!()
    }
}
