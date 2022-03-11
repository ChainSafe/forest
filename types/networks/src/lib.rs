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
mod mainnet;

/// Newest network version for all networks
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V14;

pub use self::mainnet::*;

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

pub enum Height {
    ActorsV2,
    ActorsV4,
    Liftoff,
    Ignition,
    Calico,
    Smoke,
    Hyperdrive,
}

#[async_trait]
/// Trait used as the interface to be able to support different network configuration (mainnet, calibnet, file driven)
pub trait Config {
    /// Gets network config name.
    fn name(&self) -> &str;
    /// Gets network version from epoch.
    fn network_version(&self, epoch: ChainEpoch) -> NetworkVersion;
    /// Constructs a drand beacon schedule based on the build config.
    async fn get_beacon_schedule(
        &self,
        genesis_ts: u64,
    ) -> Result<BeaconSchedule<DrandBeacon>, Box<dyn Error>>;
    /// Gets genesis car file bytes.
    fn genesis_bytes(&self) -> &[u8];
    /// Bootstrap peer ids.
    fn bootstrap_peers(&self) -> &'static [&'static str];
    /// Time, in seconds, between each block.
    fn block_delay(&self) -> u64;
    /// Gets chain epoch's height.
    fn epoch(&self, height: Height) -> ChainEpoch;
}

pub enum Network {
    Calibnet,
    Mainnet,
    Custom {
        name: Option<String>,
        bootstrap_peers: Option<&'static [&'static str]>,
        genesis_bytes: Option<&'static [u8]>,
    },
}

pub fn build_config(network: Network) -> Box<dyn Config + Send + Sync> {
    match network {
        Network::Mainnet => Box::new(MainnetConfig::new()),
        Network::Calibnet => todo!(),
        Network::Custom {
            name,
            bootstrap_peers,
            genesis_bytes,
        } => Box::new(CustomConfig::new(name, bootstrap_peers, genesis_bytes)),
    }
}

struct MainnetConfig {}

impl MainnetConfig {
    fn new() -> Self {
        MainnetConfig {}
    }
}

#[async_trait]
impl Config for MainnetConfig {
    fn name(&self) -> &str {
        "mainnet"
    }
    fn network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        VERSION_SCHEDULE
            .iter()
            .rev()
            .find(|upgrade| epoch > upgrade.height)
            .map(|upgrade| upgrade.network)
            .unwrap_or(NetworkVersion::V0)
    }
    async fn get_beacon_schedule(
        &self,
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
    fn genesis_bytes(&self) -> &'static [u8] {
        DEFAULT_GENESIS
    }
    fn bootstrap_peers(&self) -> &'static [&'static str] {
        DEFAULT_BOOTSTRAP
    }
    fn block_delay(&self) -> u64 {
        BLOCK_DELAY_SECS
    }
    fn epoch(&self, height: Height) -> ChainEpoch {
        todo!()
    }
}

struct CustomConfig {
    name: String,
    bootstrap_peers: &'static [&'static str],
    genesis_bytes: &'static [u8],
}

impl CustomConfig {
    fn new(
        name: Option<String>,
        bootstrap_peers: Option<&'static [&'static str]>,
        genesis_bytes: Option<&'static [u8]>,
    ) -> Self {
        let name = match name {
            Some(name) => name,
            None => String::from("devnet??"),
        };

        let bootstrap_peers = match bootstrap_peers {
            Some(peers) => peers,
            None => DEFAULT_BOOTSTRAP,
        };

        let genesis_bytes = match genesis_bytes {
            Some(bytes) => bytes,
            None => DEFAULT_GENESIS,
        };

        CustomConfig {
            name,
            bootstrap_peers,
            genesis_bytes,
        }
    }
}

#[async_trait]
impl Config for CustomConfig {
    fn name(&self) -> &str {
        &self.name
    }

    fn network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        VERSION_SCHEDULE
            .iter()
            .rev()
            .find(|upgrade| epoch > upgrade.height)
            .map(|upgrade| upgrade.network)
            .unwrap_or(NetworkVersion::V0)
    }

    async fn get_beacon_schedule(
        &self,
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

    fn genesis_bytes(&self) -> &'static [u8] {
        &self.genesis_bytes
    }

    fn bootstrap_peers(&self) -> &'static [&'static str] {
        &self.bootstrap_peers
    }

    fn block_delay(&self) -> u64 {
        todo!()
    }

    fn epoch(&self, height: Height) -> ChainEpoch {
        todo!()
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
