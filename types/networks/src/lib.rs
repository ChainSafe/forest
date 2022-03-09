// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

use async_trait::async_trait;
use beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use clock::ChainEpoch;
use fil_types::NetworkVersion;
use std::{error::Error, sync::Arc};

mod drand;
mod calibnet;

pub use self::calibnet::*;

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

#[async_trait]
/// Trait used as the interface to be able to support different network configuration (mainnet, calibnet, file driven)
pub trait Config {
    /// Gets network config name.
    fn name(&self) -> &str;
    /// Gets network version from epoch.
    fn network_version(&self, epoch: ChainEpoch) -> NetworkVersion;
    /// Constructs a drand beacon schedule based on the build config.
    async fn get_beacon_schedule(&self, genesis_ts: u64) -> Result<BeaconSchedule<DrandBeacon>, Box<dyn Error>>;
    /// Gets genesis car file bytes.
    fn genesis_bytes(&self) -> &'static [u8];
    /// Bootstrap peer ids.
    fn bootstrap_peers(&self) -> &'static [&'static str];
}

pub enum Network {
    Calibnet,
    Mainnet,
}

pub fn build_config(network: Network) -> Box<dyn Config + Send + Sync> {
    match network {
        Network::Calibnet => Box::new(CalibnetConfig::new()),
        Network::Mainnet => todo!(),
    }
}

struct CalibnetConfig {}

impl CalibnetConfig {
    fn new() -> Self {
        CalibnetConfig {}
    }
}

#[async_trait]
impl Config for CalibnetConfig {
    fn name(&self) -> &str {
        "calibnet"
    }
    fn network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        VERSION_SCHEDULE
            .iter()
            .rev()
            .find(|upgrade| epoch > upgrade.height)
            .map(|upgrade| upgrade.network)
            .unwrap_or(NetworkVersion::V0)
    }
    async fn get_beacon_schedule(&self, genesis_ts: u64) -> Result<BeaconSchedule<DrandBeacon>, Box<dyn Error>> {
        let mut points = BeaconSchedule(Vec::with_capacity(DRAND_SCHEDULE.len()));
        for dc in DRAND_SCHEDULE.iter() {
            points.0.push(BeaconPoint {
                height: dc.height,
                beacon: Arc::new(DrandBeacon::new(genesis_ts, BLOCK_DELAY_SECS, dc.config).await?),
            });
        }
        Ok(points)
    }
    fn genesis_bytes(&self) -> &'static [u8] {
        DEFAULT_GENESIS
    }
    fn bootstrap_peers(&self) -> &'static [&'static str] {
        DEFAULT_BOOTSTRAP
    }
}

const VERSION_SCHEDULE: [Upgrade; 14] = [
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
    },
];
