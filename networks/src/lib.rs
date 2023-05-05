// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{str::FromStr, sync::Arc};

use anyhow::Error;
use cid::Cid;
use fil_actors_runtime_v10::runtime::Policy;
use forest_beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use forest_shim::version::NetworkVersion;
use fvm_shared::clock::{ChainEpoch, EPOCH_DURATION_SECONDS};
use serde::{Deserialize, Serialize};
use strum_macros::Display;
use url::Url;

pub mod calibnet;
mod drand;
pub mod mainnet;

// As per https://github.com/ethereum-lists/chains
// https://github.com/ethereum-lists/chains/blob/4731f6713c6fc2bf2ae727388642954a6545b3a9/_data/chains/eip155-314.json
const MAINNET_ETH_CHAIN_ID: u64 = 314;
// https://github.com/ethereum-lists/chains/blob/4731f6713c6fc2bf2ae727388642954a6545b3a9/_data/chains/eip155-314159.json
const CALIBNET_ETH_CHAIN_ID: u64 = 314159;

/// Newest network version for all networks
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V17;

const DEFAULT_RECENT_STATE_ROOTS: i64 = 2000;

/// Forest builtin `filecoin` network chains. In general only `mainnet` and its
/// chain information should be considered stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkChain {
    Mainnet,
    Calibnet,
}

impl FromStr for NetworkChain {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(NetworkChain::Mainnet),
            "calibnet" => Ok(NetworkChain::Calibnet),
            name => Err(anyhow::anyhow!("unsupported network chain: {name}")),
        }
    }
}

/// Defines the meaningful heights of the protocol.
#[derive(Debug, Display, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
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
    Trust,
    Norwegian,
    Turbo,
    Hyperdrive,
    Chocolate,
    OhSnap,
    Skyr,
    Shark,
    Hygge,
    Lightning,
    Thunder,
}

impl Default for Height {
    fn default() -> Height {
        Self::Breeze
    }
}

impl From<Height> for NetworkVersion {
    fn from(height: Height) -> NetworkVersion {
        match height {
            Height::Breeze => NetworkVersion::V1,
            Height::Smoke => NetworkVersion::V2,
            Height::Ignition => NetworkVersion::V3,
            Height::ActorsV2 => NetworkVersion::V4,
            Height::Tape => NetworkVersion::V5,
            Height::Liftoff => NetworkVersion::V5,
            Height::Kumquat => NetworkVersion::V6,
            Height::Calico => NetworkVersion::V7,
            Height::Persian => NetworkVersion::V8,
            Height::Orange => NetworkVersion::V9,
            Height::Trust => NetworkVersion::V10,
            Height::Norwegian => NetworkVersion::V11,
            Height::Turbo => NetworkVersion::V12,
            Height::Hyperdrive => NetworkVersion::V13,
            Height::Chocolate => NetworkVersion::V14,
            Height::OhSnap => NetworkVersion::V15,
            Height::Skyr => NetworkVersion::V16,
            Height::Shark => NetworkVersion::V17,
            Height::Hygge => NetworkVersion::V18,
            Height::Lightning => NetworkVersion::V19,
            Height::Thunder => NetworkVersion::V20,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ActorBundleInfo {
    pub manifest: Cid,
    pub url: Url,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct HeightInfo {
    pub height: Height,
    pub epoch: ChainEpoch,
    pub bundle: Option<ActorBundleInfo>,
}

pub fn sort_by_epoch(height_info_slice: &[HeightInfo]) -> Vec<HeightInfo> {
    let mut height_info_vec = height_info_slice.to_vec();
    height_info_vec.sort_by(|a, b| a.epoch.cmp(&b.epoch));
    height_info_vec
}

#[derive(Clone)]
struct DrandPoint<'a> {
    pub height: ChainEpoch,
    pub config: &'a DrandConfig<'a>,
}

/// Defines all network configuration parameters.
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ChainConfig {
    pub name: String,
    pub genesis_cid: Option<String>,
    pub bootstrap_peers: Vec<String>,
    pub block_delay_secs: u64,
    pub height_infos: Vec<HeightInfo>,
    #[serde(default = "default_policy")]
    pub policy: Policy,
    pub eth_chain_id: u64,
    /// Number of default recent state roots to keep in memory and include in
    /// the exported snapshot.
    pub recent_state_roots: i64,
}

impl ChainConfig {
    pub fn mainnet() -> Self {
        use mainnet::*;
        Self {
            name: "mainnet".to_string(),
            genesis_cid: Some(GENESIS_CID.to_owned()),
            bootstrap_peers: DEFAULT_BOOTSTRAP.iter().map(|x| x.to_string()).collect(),
            block_delay_secs: EPOCH_DURATION_SECONDS as u64,
            height_infos: HEIGHT_INFOS.to_vec(),
            policy: Policy::mainnet(),
            eth_chain_id: MAINNET_ETH_CHAIN_ID,
            recent_state_roots: DEFAULT_RECENT_STATE_ROOTS,
        }
    }

    pub fn calibnet() -> Self {
        use calibnet::*;
        Self {
            name: "calibnet".to_string(),
            genesis_cid: Some(GENESIS_CID.to_owned()),
            bootstrap_peers: DEFAULT_BOOTSTRAP.iter().map(|x| x.to_string()).collect(),
            block_delay_secs: EPOCH_DURATION_SECONDS as u64,
            height_infos: HEIGHT_INFOS.to_vec(),
            policy: Policy::calibnet(),
            eth_chain_id: CALIBNET_ETH_CHAIN_ID,
            recent_state_roots: DEFAULT_RECENT_STATE_ROOTS,
        }
    }

    pub fn from_chain(network_chain: &NetworkChain) -> Self {
        match network_chain {
            NetworkChain::Mainnet => Self::mainnet(),
            NetworkChain::Calibnet => Self::calibnet(),
        }
    }

    pub fn network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        let height = sort_by_epoch(&self.height_infos)
            .iter()
            .rev()
            .find(|info| epoch > info.epoch)
            .map(|info| info.height)
            .unwrap_or(Height::Breeze);

        From::from(height)
    }

    pub fn get_beacon_schedule(
        &self,
        genesis_ts: u64,
    ) -> Result<BeaconSchedule<DrandBeacon>, anyhow::Error> {
        let ds_iter = if self.name == "calibnet" {
            calibnet::DRAND_SCHEDULE.iter()
        } else {
            mainnet::DRAND_SCHEDULE.iter()
        };
        let mut points = BeaconSchedule::with_capacity(ds_iter.len());
        for dc in ds_iter {
            points.0.push(BeaconPoint {
                height: dc.height,
                beacon: Arc::new(DrandBeacon::new(
                    genesis_ts,
                    self.block_delay_secs,
                    dc.config,
                )?),
            });
        }
        Ok(points)
    }

    pub fn epoch(&self, height: Height) -> ChainEpoch {
        sort_by_epoch(&self.height_infos)
            .iter()
            .find(|info| height == info.height)
            .map(|info| info.epoch)
            .unwrap_or(0)
    }

    pub fn genesis_bytes(&self) -> Option<&[u8]> {
        match self.name.as_ref() {
            "mainnet" => Some(mainnet::DEFAULT_GENESIS),
            "calibnet" => Some(calibnet::DEFAULT_GENESIS),
            _ => None,
        }
    }

    pub fn is_testnet(&self) -> bool {
        !matches!(self.name.as_ref(), "mainnet")
    }
}

impl Default for ChainConfig {
    fn default() -> Self {
        ChainConfig::mainnet()
    }
}

// XXX: Dummy default. Will be overwritten later. Wish we could get rid of this.
fn default_policy() -> Policy {
    Policy::mainnet()
}
