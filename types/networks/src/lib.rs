// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

use beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use clock::ChainEpoch;
use fil_types::NetworkVersion;
use std::{error::Error, sync::Arc};

mod drand;
mod calibnet;
mod mainnet;

/// Newest network version for all networks
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V14;

/// Defines the different hard fork parameters.
pub struct Upgrade {
    /// When the hard fork will happen
    height: ChainEpoch,
    /// The version of the fork
    network: NetworkVersion,
}

#[derive(Clone)]
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
        use mainnet::*;
        Self {
            name: "mainnet".to_string(),
            version_schedule: VERSION_SCHEDULE,
            drand_schedule: DRAND_SCHEDULE.to_vec(),
            genesis_bytes: DEFAULT_GENESIS.to_vec(),
            bootstrap_peers: DEFAULT_BOOTSTRAP.iter().map(|x| x.to_string()).collect(),
            block_delay_secs: BLOCK_DELAY_SECS,
        }
    }

    pub fn calibnet() -> Self {
        use calibnet::*;
        Self {
            name: "calibnet".to_string(),
            version_schedule: VERSION_SCHEDULE,
            drand_schedule: DRAND_SCHEDULE.to_vec(),
            genesis_bytes: DEFAULT_GENESIS.to_vec(),
            bootstrap_peers: DEFAULT_BOOTSTRAP.iter().map(|x| x.to_string()).collect(),
            block_delay_secs: BLOCK_DELAY_SECS,
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
