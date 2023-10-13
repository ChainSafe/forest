// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{fmt::Display, str::FromStr};

use cid::Cid;
use fil_actors_shared::v10::runtime::Policy;
use libp2p::Multiaddr;
use once_cell::sync::Lazy;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use strum_macros::Display;

use crate::beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use crate::shim::clock::{ChainEpoch, EPOCH_DURATION_SECONDS};
use crate::shim::sector::{RegisteredPoStProofV3, RegisteredSealProofV3};
use crate::shim::version::NetworkVersion;

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
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[serde(tag = "type", content = "name", rename_all = "lowercase")]
pub enum NetworkChain {
    Mainnet,
    Calibnet,
    Devnet(String),
}

impl FromStr for NetworkChain {
    type Err = anyhow::Error;

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
    /// Returns [`NetworkChain::Calibnet`] or [`NetworkChain::Mainnet`] if `cid`
    /// is the hard-coded genesis CID for either of those networks.
    pub fn from_genesis(cid: &Cid) -> Option<Self> {
        match (
            *calibnet::GENESIS_CID == *cid,
            *mainnet::GENESIS_CID == *cid,
        ) {
            (true, true) => unreachable!(),
            (true, false) => Some(Self::Calibnet),
            (false, true) => Some(Self::Mainnet),
            (false, false) => None,
        }
    }

    /// Returns [`NetworkChain::Calibnet`] or [`NetworkChain::Mainnet`] if `cid`
    /// is the hard-coded genesis CID for either of those networks.
    ///
    /// Else returns a [`NetworkChain::Devnet`] with a placeholder name.
    pub fn from_genesis_or_devnet_placeholder(cid: &Cid) -> Self {
        Self::from_genesis(cid).unwrap_or(Self::Devnet(String::from("devnet")))
    }

    pub fn is_testnet(&self) -> bool {
        !matches!(self, NetworkChain::Mainnet)
    }
}

/// Defines the meaningful heights of the protocol.
#[derive(Debug, Display, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
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
    Watermelon,
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
            Height::Watermelon => NetworkVersion::V21,
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct HeightInfo {
    pub height: Height,
    pub epoch: ChainEpoch,
    pub bundle: Option<Cid>,
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
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[serde(default)]
pub struct ChainConfig {
    pub network: NetworkChain,
    pub genesis_cid: Option<String>,
    #[cfg_attr(test, arbitrary(gen(
        |g: &mut quickcheck::Gen| {
            let addr = std::net::Ipv4Addr::arbitrary(&mut *g);
            let n = u8::arbitrary(g) as usize;
            vec![addr.into(); n]
        }
    )))]
    pub bootstrap_peers: Vec<Multiaddr>,
    pub block_delay_secs: u32,
    pub propagation_delay_secs: u32,
    pub height_infos: Vec<HeightInfo>,
    #[cfg_attr(test, arbitrary(gen(|_g| Policy::mainnet())))]
    #[serde(default = "default_policy")]
    pub policy: Policy,
    pub eth_chain_id: u32,
    /// Number of default recent state roots to keep in memory and include in
    /// the exported snapshot.
    pub recent_state_roots: i64,
    pub request_window: u32,
}

impl ChainConfig {
    pub fn mainnet() -> Self {
        use mainnet::*;
        Self {
            network: NetworkChain::Mainnet,
            genesis_cid: Some(GENESIS_CID.to_string()),
            bootstrap_peers: DEFAULT_BOOTSTRAP.clone(),
            block_delay_secs: EPOCH_DURATION_SECONDS as u32,
            propagation_delay_secs: 10,
            height_infos: HEIGHT_INFOS.to_vec(),
            policy: Policy::mainnet(),
            eth_chain_id: ETH_CHAIN_ID as u32,
            recent_state_roots: DEFAULT_RECENT_STATE_ROOTS,
            request_window: DEFAULT_REQUEST_WINDOW as u32,
        }
    }

    pub fn calibnet() -> Self {
        use calibnet::*;
        Self {
            network: NetworkChain::Calibnet,
            genesis_cid: Some(GENESIS_CID.to_string()),
            bootstrap_peers: DEFAULT_BOOTSTRAP.clone(),
            block_delay_secs: EPOCH_DURATION_SECONDS as u32,
            propagation_delay_secs: 10,
            height_infos: HEIGHT_INFOS.to_vec(),
            policy: Policy::calibnet(),
            eth_chain_id: ETH_CHAIN_ID as u32,
            recent_state_roots: DEFAULT_RECENT_STATE_ROOTS,
            request_window: DEFAULT_REQUEST_WINDOW as u32,
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
            RegisteredSealProofV3::StackedDRG2KiBV1,
            RegisteredSealProofV3::StackedDRG8MiBV1,
        ]);
        policy.valid_pre_commit_proof_type = allowed_proof_types;
        #[allow(clippy::disallowed_types)]
        let allowed_proof_types = std::collections::HashSet::from_iter(vec![
            RegisteredPoStProofV3::StackedDRGWindow2KiBV1,
            RegisteredPoStProofV3::StackedDRGWindow8MiBV1,
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
            eth_chain_id: ETH_CHAIN_ID as u32,
            recent_state_roots: DEFAULT_RECENT_STATE_ROOTS,
            request_window: DEFAULT_REQUEST_WINDOW as u32,
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

    pub fn get_beacon_schedule(&self, genesis_ts: u64) -> BeaconSchedule {
        let ds_iter = match self.network {
            NetworkChain::Mainnet => mainnet::DRAND_SCHEDULE.iter(),
            NetworkChain::Calibnet => calibnet::DRAND_SCHEDULE.iter(),
            NetworkChain::Devnet(_) => devnet::DRAND_SCHEDULE.iter(),
        };

        BeaconSchedule(
            ds_iter
                .map(|dc| BeaconPoint {
                    height: dc.height,
                    beacon: Box::new(DrandBeacon::new(
                        genesis_ts,
                        self.block_delay_secs as u64,
                        dc.config,
                    )),
                })
                .collect(),
        )
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
        self.network.is_testnet()
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

pub(crate) fn parse_bootstrap_peers(bootstrap_peer_list: &str) -> Vec<Multiaddr> {
    bootstrap_peer_list
        .split('\n')
        .filter(|s| !s.is_empty())
        .map(|s| {
            Multiaddr::from_str(s).unwrap_or_else(|e| panic!("invalid bootstrap peer {s}: {e}"))
        })
        .collect()
}

#[derive(Debug)]
pub struct ActorBundleInfo {
    pub manifest: Cid,
    pub url: Url,
}

macro_rules! actor_bundle_info {
    ($($cid:literal @ $version:literal for $network:literal),* $(,)?) => {
        [
            $(
                ActorBundleInfo {
                    manifest: $cid.parse().unwrap(),
                    url: concat!(
                            "https://github.com/filecoin-project/builtin-actors/releases/download/",
                            $version,
                            "/builtin-actors-",
                            $network,
                            ".car"
                        ).parse().unwrap()
                },
            )*
        ]
    }
}

pub static ACTOR_BUNDLES: Lazy<Box<[ActorBundleInfo]>> = Lazy::new(|| {
    Box::new(actor_bundle_info![
        "bafy2bzacedbedgynklc4dgpyxippkxmba2mgtw7ecntoneclsvvl4klqwuyyy" @ "v9.0.3" for "calibrationnet",
        "bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo" @ "v10.0.0-rc.1" for "calibrationnet",
        "bafy2bzacedhuowetjy2h4cxnijz2l64h4mzpk5m256oywp4evarpono3cjhco" @ "v11.0.0-rc2" for "calibrationnet",
        "bafy2bzacedozk3jh2j4nobqotkbofodq4chbrabioxbfrygpldgoxs3zwgggk" @ "v9.0.3" for "devnet",
        "bafy2bzacebzz376j5kizfck56366kdz5aut6ktqrvqbi3efa2d4l2o2m653ts" @ "v10.0.0" for "devnet",
        "bafy2bzaceay35go4xbjb45km6o46e5bib3bi46panhovcbedrynzwmm3drr4i" @ "v11.0.0" for "devnet",
        "bafy2bzacebk6yiirh4ennphzyka7b6g6jzn3lt4lr5ht7rjwulnrcthjihapo" @ "v12.0.0-rc.1" for "devnet",
        "bafy2bzaceb6j6666h36xnhksu3ww4kxb6e25niayfgkdnifaqi6m6ooc66i6i" @ "v9.0.3" for "mainnet",
        "bafy2bzacecsuyf7mmvrhkx2evng5gnz5canlnz2fdlzu2lvcgptiq2pzuovos" @ "v10.0.0" for "mainnet",
        "bafy2bzacecnhaiwcrpyjvzl4uv4q3jzoif26okl3m66q3cijp3dfwlcxwztwo" @ "v11.0.0" for "mainnet",
    ])
});
