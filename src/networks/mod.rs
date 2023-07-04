// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{fmt::Display, str::FromStr, sync::Arc};

use crate::beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use crate::shim::clock::{ChainEpoch, EPOCH_DURATION_SECONDS};
use crate::shim::sector::{RegisteredPoStProof, RegisteredSealProof};
use crate::shim::version::NetworkVersion;
use crate::shim::Inner;
use anyhow::Error;
use cid::Cid;
use fil_actors_shared::v10::runtime::Policy;
use serde::{Deserialize, Serialize};
use strum_macros::Display;
use url::Url;

mod drand;

pub mod calibnet;
pub mod devnet;
pub mod mainnet;

/// Newest network version for all networks
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V17;

const DEFAULT_RECENT_STATE_ROOTS: i64 = 2000;

// Sync the messages for one or many tipsets @ a time
// Lotus uses a window size of 8: https://github.com/filecoin-project/lotus/blob/c1d22d8b3298fdce573107413729be608e72187d/chain/sync.go#L56
const DEFAULT_REQUEST_WINDOW: usize = 8;

/// Forest builtin `filecoin` network chains. In general only `mainnet` and its
/// chain information should be considered stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "name", rename_all = "lowercase")]
pub enum NetworkChain {
    Mainnet,
    Calibnet,
    Devnet(String),
}

impl FromStr for NetworkChain {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(NetworkChain::Mainnet),
            "calibnet" => Ok(NetworkChain::Calibnet),
            name => Ok(NetworkChain::Devnet(name.to_owned())),
        }
    }
}

impl Display for NetworkChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkChain::Mainnet => write!(f, "mainnet"),
            NetworkChain::Calibnet => write!(f, "calibnet"),
            NetworkChain::Devnet(name) => write!(f, "{name}"),
        }
    }
}

impl NetworkChain {
    pub fn is_devnet(&self) -> bool {
        matches!(self, NetworkChain::Devnet(_))
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
#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct ChainConfig {
    pub network: NetworkChain,
    pub genesis_cid: Option<String>,
    pub bootstrap_peers: Vec<String>,
    pub block_delay_secs: u64,
    pub propagation_delay_secs: u64,
    pub height_infos: Vec<HeightInfo>,
    #[serde(default = "default_policy")]
    pub policy: Policy,
    pub eth_chain_id: u64,
    /// Number of default recent state roots to keep in memory and include in
    /// the exported snapshot.
    pub recent_state_roots: i64,
    pub request_window: usize,
}

impl ChainConfig {
    pub fn mainnet() -> Self {
        use mainnet::*;
        Self {
            network: NetworkChain::Mainnet,
            genesis_cid: Some(GENESIS_CID.to_owned()),
            bootstrap_peers: DEFAULT_BOOTSTRAP.iter().map(|x| x.to_string()).collect(),
            block_delay_secs: EPOCH_DURATION_SECONDS as u64,
            propagation_delay_secs: 10,
            height_infos: HEIGHT_INFOS.to_vec(),
            policy: Policy::mainnet(),
            eth_chain_id: ETH_CHAIN_ID,
            recent_state_roots: DEFAULT_RECENT_STATE_ROOTS,
            request_window: DEFAULT_REQUEST_WINDOW,
        }
    }

    pub fn calibnet() -> Self {
        use calibnet::*;
        Self {
            network: NetworkChain::Calibnet,
            genesis_cid: Some(GENESIS_CID.to_owned()),
            bootstrap_peers: DEFAULT_BOOTSTRAP.iter().map(|x| x.to_string()).collect(),
            block_delay_secs: EPOCH_DURATION_SECONDS as u64,
            propagation_delay_secs: 10,
            height_infos: HEIGHT_INFOS.to_vec(),
            policy: Policy::calibnet(),
            eth_chain_id: ETH_CHAIN_ID,
            recent_state_roots: DEFAULT_RECENT_STATE_ROOTS,
            request_window: DEFAULT_REQUEST_WINDOW,
        }
    }

    pub fn devnet() -> Self {
        use devnet::*;
        let mut policy = Policy::mainnet();
        policy.minimum_consensus_power = 2048.into();
        policy.minimum_verified_allocation_size = 256.into();
        policy.pre_commit_challenge_delay = 10;

        #[allow(clippy::disallowed_types)]
        let allowed_proof_types = std::collections::HashSet::from_iter(vec![
            <RegisteredSealProof as Inner>::FVM::StackedDRG2KiBV1,
            <RegisteredSealProof as Inner>::FVM::StackedDRG8MiBV1,
        ]);
        policy.valid_pre_commit_proof_type = allowed_proof_types;
        #[allow(clippy::disallowed_types)]
        let allowed_proof_types = std::collections::HashSet::from_iter(vec![
            <RegisteredPoStProof as Inner>::FVM::StackedDRGWindow2KiBV1,
            <RegisteredPoStProof as Inner>::FVM::StackedDRGWindow8MiBV1,
        ]);
        policy.valid_post_proof_type = allowed_proof_types;

        Self {
            network: NetworkChain::Devnet("devnet".to_string()),
            genesis_cid: None,
            bootstrap_peers: Vec::new(),
            block_delay_secs: 4,
            propagation_delay_secs: 1,
            height_infos: HEIGHT_INFOS.to_vec(),
            policy,
            eth_chain_id: ETH_CHAIN_ID,
            recent_state_roots: DEFAULT_RECENT_STATE_ROOTS,
            request_window: DEFAULT_REQUEST_WINDOW,
        }
    }

    pub fn from_chain(network_chain: &NetworkChain) -> Self {
        match network_chain {
            NetworkChain::Mainnet => Self::mainnet(),
            NetworkChain::Calibnet => Self::calibnet(),
            NetworkChain::Devnet(name) => Self {
                network: NetworkChain::Devnet(name.clone()),
                ..Self::devnet()
            },
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
        let ds_iter = match self.network {
            NetworkChain::Mainnet => mainnet::DRAND_SCHEDULE.iter(),
            NetworkChain::Calibnet => calibnet::DRAND_SCHEDULE.iter(),
            NetworkChain::Devnet(_) => devnet::DRAND_SCHEDULE.iter(),
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
        match self.network {
            NetworkChain::Mainnet => Some(mainnet::DEFAULT_GENESIS),
            NetworkChain::Calibnet => Some(calibnet::DEFAULT_GENESIS),
            NetworkChain::Devnet(_) => None,
        }
    }

    pub fn is_testnet(&self) -> bool {
        !matches!(self.network, NetworkChain::Mainnet)
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
