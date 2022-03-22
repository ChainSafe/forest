// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

use beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use clock::{ChainEpoch, EPOCH_DURATION_SECONDS};
use fil_types::NetworkVersion;
use serde::{Deserialize, Serialize};
use std::{error::Error, sync::Arc};

mod calibnet;
mod drand;
mod mainnet;

/// Newest network version for all networks
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V14;

const UPGRADE_INFOS: [UpgradeInfo; 14] = [
    UpgradeInfo {
        height: Height::Breeze,
        version: 1,
    },
    UpgradeInfo {
        height: Height::Smoke,
        version: 2,
    },
    UpgradeInfo {
        height: Height::Ignition,
        version: 3,
    },
    UpgradeInfo {
        height: Height::ActorsV2,
        version: 4,
    },
    UpgradeInfo {
        height: Height::Tape,
        version: 5,
    },
    UpgradeInfo {
        height: Height::Kumquat,
        version: 6,
    },
    UpgradeInfo {
        height: Height::Calico,
        version: 7,
    },
    UpgradeInfo {
        height: Height::Persian,
        version: 8,
    },
    UpgradeInfo {
        height: Height::Orange,
        version: 9,
    },
    UpgradeInfo {
        height: Height::Trust,
        version: 10,
    },
    UpgradeInfo {
        height: Height::Norwegian,
        version: 11,
    },
    UpgradeInfo {
        height: Height::Turbo,
        version: 12,
    },
    UpgradeInfo {
        height: Height::Hyperdrive,
        version: 13,
    },
    UpgradeInfo {
        height: Height::Chocolate,
        version: 14,
    },
];

const MAINNET_HEIGHT_INFOS: [HeightInfo; 17] = [
    HeightInfo {
        height: Height::Breeze,
        epoch: 41280,
    },
    HeightInfo {
        height: Height::Smoke,
        epoch: 51000,
    },
    HeightInfo {
        height: Height::Ignition,
        epoch: 94000,
    },
    HeightInfo {
        height: Height::ActorsV2,
        epoch: 138720,
    },
    HeightInfo {
        height: Height::Tape,
        epoch: 140760,
    },
    HeightInfo {
        height: Height::Liftoff,
        epoch: 148888,
    },
    HeightInfo {
        height: Height::Kumquat,
        epoch: 170000,
    },
    HeightInfo {
        height: Height::Calico,
        epoch: 265200,
    },
    HeightInfo {
        height: Height::Persian,
        epoch: 272400,
    },
    HeightInfo {
        height: Height::Orange,
        epoch: 336458,
    },
    HeightInfo {
        height: Height::Claus,
        epoch: 343200,
    },
    HeightInfo {
        height: Height::Trust,
        epoch: 550321,
    },
    HeightInfo {
        height: Height::Norwegian,
        epoch: 665280,
    },
    HeightInfo {
        height: Height::Turbo,
        epoch: 712320,
    },
    HeightInfo {
        height: Height::Hyperdrive,
        epoch: 892800,
    },
    HeightInfo {
        height: Height::Chocolate,
        epoch: 1231620,
    },
    HeightInfo {
        height: Height::OhSnap,
        epoch: 9999999,
    },
];

const CALIBNET_HEIGHT_INFOS: [HeightInfo; 17] = [
    HeightInfo {
        height: Height::Breeze,
        epoch: -1,
    },
    HeightInfo {
        height: Height::Smoke,
        epoch: -2,
    },
    HeightInfo {
        height: Height::Ignition,
        epoch: -3,
    },
    HeightInfo {
        height: Height::ActorsV2,
        epoch: 30,
    },
    HeightInfo {
        height: Height::Tape,
        epoch: 60,
    },
    HeightInfo {
        height: Height::Liftoff,
        epoch: -5,
    },
    HeightInfo {
        height: Height::Kumquat,
        epoch: 90,
    },
    HeightInfo {
        height: Height::Calico,
        epoch: 120,
    },
    HeightInfo {
        height: Height::Persian,
        epoch: 130,
    },
    HeightInfo {
        height: Height::Orange,
        epoch: 300,
    },
    HeightInfo {
        height: Height::Claus,
        epoch: 270,
    },
    HeightInfo {
        height: Height::Trust,
        epoch: 330,
    },
    HeightInfo {
        height: Height::Norwegian,
        epoch: 360,
    },
    HeightInfo {
        height: Height::Turbo,
        epoch: 390,
    },
    HeightInfo {
        height: Height::Hyperdrive,
        epoch: 420,
    },
    HeightInfo {
        height: Height::Chocolate,
        epoch: 312746,
    },
    HeightInfo {
        height: Height::OhSnap,
        epoch: 9999999,
    },
];

const CONFORMANCE_HEIGHT_INFOS: [HeightInfo; 17] = [
    HeightInfo {
        height: Height::Breeze,
        epoch: -15,
    },
    HeightInfo {
        height: Height::Smoke,
        epoch: -14,
    },
    HeightInfo {
        height: Height::Ignition,
        epoch: -13,
    },
    HeightInfo {
        height: Height::ActorsV2,
        epoch: -12,
    },
    HeightInfo {
        height: Height::Tape,
        epoch: -11,
    },
    HeightInfo {
        height: Height::Liftoff,
        epoch: -10,
    },
    HeightInfo {
        height: Height::Kumquat,
        epoch: -9,
    },
    HeightInfo {
        height: Height::Calico,
        epoch: -8,
    },
    HeightInfo {
        height: Height::Persian,
        epoch: -7,
    },
    HeightInfo {
        height: Height::Orange,
        epoch: -6,
    },
    HeightInfo {
        height: Height::Claus,
        epoch: -5,
    },
    HeightInfo {
        height: Height::Trust,
        epoch: -4,
    },
    HeightInfo {
        height: Height::Norwegian,
        epoch: -3,
    },
    HeightInfo {
        height: Height::Turbo,
        epoch: -2,
    },
    HeightInfo {
        height: Height::Hyperdrive,
        epoch: -1,
    },
    HeightInfo {
        height: Height::Chocolate,
        epoch: -16,
    },
    HeightInfo {
        height: Height::OhSnap,
        epoch: -17,
    },
];

/// Defines the meaningful heights of the protocol.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
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

impl Default for Height {
    fn default() -> Height {
        Self::Breeze
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct UpgradeInfo {
    height: Height,
    version: u32,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HeightInfo {
    pub height: Height,
    pub epoch: ChainEpoch,
}

#[derive(Clone)]
struct DrandPoint<'a> {
    pub height: ChainEpoch,
    pub config: &'a DrandConfig<'a>,
}

/// Defines all network configuration parameters.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct ChainConfig {
    pub name: String,
    pub bootstrap_peers: Vec<String>,
    pub block_delay_secs: u64,
    #[serde(skip_serializing)]
    pub genesis_bytes: Vec<u8>,
    pub version_schedule: Vec<UpgradeInfo>,
    pub height_infos: Vec<HeightInfo>,
}

impl ChainConfig {
    pub fn calibnet() -> Self {
        use calibnet::*;
        Self {
            name: "calibnet".to_string(),
            bootstrap_peers: DEFAULT_BOOTSTRAP.iter().map(|x| x.to_string()).collect(),
            block_delay_secs: EPOCH_DURATION_SECONDS as u64,
            genesis_bytes: DEFAULT_GENESIS.to_vec(),
            version_schedule: UPGRADE_INFOS.to_vec(),
            height_infos: CALIBNET_HEIGHT_INFOS.to_vec(),
        }
    }

    pub fn conformance() -> Self {
        Self {
            height_infos: CONFORMANCE_HEIGHT_INFOS.to_vec(),
            ..Self::default()
        }
    }

    pub fn network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        let height = self
            .height_infos
            .iter()
            .rev()
            .find(|info| epoch > info.epoch)
            .map(|info| info.height)
            .unwrap_or(Height::Breeze);

        let version = self
            .version_schedule
            .iter()
            .find(|info| height == info.height)
            .map(|info| info.version)
            .unwrap();

        NetworkVersion::try_from(version).unwrap()
    }

    pub async fn get_beacon_schedule(
        &self,
        genesis_ts: u64,
    ) -> Result<BeaconSchedule<DrandBeacon>, Box<dyn Error>> {
        if self.name == "calibnet" {
            let mut points = BeaconSchedule(Vec::with_capacity(calibnet::DRAND_SCHEDULE.len()));
            for dc in calibnet::DRAND_SCHEDULE.iter() {
                points.0.push(BeaconPoint {
                    height: dc.height,
                    beacon: Arc::new(
                        DrandBeacon::new(genesis_ts, self.block_delay_secs, dc.config).await?,
                    ),
                });
            }
            Ok(points)
        } else {
            let mut points = BeaconSchedule(Vec::with_capacity(mainnet::DRAND_SCHEDULE.len()));
            for dc in mainnet::DRAND_SCHEDULE.iter() {
                points.0.push(BeaconPoint {
                    height: dc.height,
                    beacon: Arc::new(
                        DrandBeacon::new(genesis_ts, self.block_delay_secs, dc.config).await?,
                    ),
                });
            }
            Ok(points)
        }
    }

    pub fn epoch(&self, height: Height) -> ChainEpoch {
        self.height_infos
            .iter()
            .find(|info| height == info.height)
            .map(|info| info.epoch)
            .unwrap()
    }
}

impl Default for ChainConfig {
    fn default() -> Self {
        use mainnet::*;
        Self {
            name: "mainnet".to_string(),
            bootstrap_peers: DEFAULT_BOOTSTRAP.iter().map(|x| x.to_string()).collect(),
            block_delay_secs: EPOCH_DURATION_SECONDS as u64,
            genesis_bytes: DEFAULT_GENESIS.to_vec(),
            version_schedule: UPGRADE_INFOS.to_vec(),
            height_infos: MAINNET_HEIGHT_INFOS.to_vec(),
        }
    }
}
