// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;
use std::sync::LazyLock;

use ahash::HashMap;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use libp2p::Multiaddr;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, IntoEnumIterator};
use tracing::warn;

use crate::beacon::{BeaconPoint, BeaconSchedule, DrandBeacon, DrandConfig};
use crate::db::SettingsStore;
use crate::eth::EthChainId;
use crate::shim::{
    clock::{ChainEpoch, EPOCH_DURATION_SECONDS, EPOCHS_IN_DAY},
    econ::TokenAmount,
    machine::BuiltinActorManifest,
    runtime::Policy,
    sector::{RegisteredPoStProofV3, RegisteredSealProofV3},
    version::NetworkVersion,
};
use crate::utils::misc::env::env_or_default;
use crate::{make_butterfly_policy, make_calibnet_policy, make_devnet_policy, make_mainnet_policy};

pub use network_name::{GenesisNetworkName, StateNetworkName};

mod actors_bundle;
pub use actors_bundle::{
    ACTOR_BUNDLES, ACTOR_BUNDLES_METADATA, ActorBundleInfo, ActorBundleMetadata,
    generate_actor_bundle, get_actor_bundles_metadata,
};

mod drand;

pub mod network_name;

pub mod butterflynet;
pub mod calibnet;
pub mod devnet;
pub mod mainnet;

pub mod metrics;

/// Newest network version for all networks
pub const NEWEST_NETWORK_VERSION: NetworkVersion = NetworkVersion::V25;

const ENV_FOREST_BLOCK_DELAY_SECS: &str = "FOREST_BLOCK_DELAY_SECS";
const ENV_FOREST_PROPAGATION_DELAY_SECS: &str = "FOREST_PROPAGATION_DELAY_SECS";
const ENV_PLEDGE_RULE_RAMP: &str = "FOREST_PLEDGE_RULE_RAMP";

static INITIAL_FIL_RESERVED: LazyLock<TokenAmount> =
    LazyLock::new(|| TokenAmount::from_whole(300_000_000));

/// Forest builtin `filecoin` network chains. In general only `mainnet` and its
/// chain information should be considered stable.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, Hash, derive_more::Display,
)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[serde(tag = "type", content = "name", rename_all = "lowercase")]
#[display(rename_all = "lowercase")]
pub enum NetworkChain {
    #[default]
    Mainnet,
    Calibnet,
    Butterflynet,
    Devnet(String),
}

impl FromStr for NetworkChain {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            mainnet::NETWORK_COMMON_NAME | mainnet::NETWORK_GENESIS_NAME => {
                Ok(NetworkChain::Mainnet)
            }
            calibnet::NETWORK_COMMON_NAME | calibnet::NETWORK_GENESIS_NAME => {
                Ok(NetworkChain::Calibnet)
            }
            butterflynet::NETWORK_COMMON_NAME => Ok(NetworkChain::Butterflynet),
            name => Ok(NetworkChain::Devnet(name.to_owned())),
        }
    }
}

impl NetworkChain {
    /// Returns the `NetworkChain`s internal name as set in the genesis block, which is not the
    /// same as the recent state network name.
    ///
    /// As a rule of thumb, the internal name should be used when interacting with
    /// protocol internals and P2P.
    pub fn genesis_name(&self) -> GenesisNetworkName {
        match self {
            NetworkChain::Mainnet => mainnet::NETWORK_GENESIS_NAME.into(),
            NetworkChain::Calibnet => calibnet::NETWORK_GENESIS_NAME.into(),
            _ => self.to_string().into(),
        }
    }
    /// Returns [`NetworkChain::Calibnet`] or [`NetworkChain::Mainnet`] if `cid`
    /// is the hard-coded genesis CID for either of those networks.
    pub fn from_genesis(cid: &Cid) -> Option<Self> {
        if cid == &*mainnet::GENESIS_CID {
            Some(Self::Mainnet)
        } else if cid == &*calibnet::GENESIS_CID {
            Some(Self::Calibnet)
        } else if cid == &*butterflynet::GENESIS_CID {
            Some(Self::Butterflynet)
        } else {
            None
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

    pub fn is_devnet(&self) -> bool {
        matches!(self, NetworkChain::Devnet(..))
    }
}

/// Defines the meaningful heights of the protocol.
#[derive(
    Debug, Default, Display, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, EnumIter,
)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub enum Height {
    #[default]
    Breeze,
    Smoke,
    Ignition,
    Refuel,
    Assembly,
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
    Skyr,
    Shark,
    Hygge,
    Lightning,
    Thunder,
    Watermelon,
    WatermelonFix,
    WatermelonFix2,
    Dragon,
    DragonFix,
    Phoenix,
    Waffle,
    TukTuk,
    Teep,
    Tock,
    TockFix,
    GoldenWeek,
}

impl From<Height> for NetworkVersion {
    fn from(height: Height) -> NetworkVersion {
        match height {
            Height::Breeze => NetworkVersion::V1,
            Height::Smoke => NetworkVersion::V2,
            Height::Ignition => NetworkVersion::V3,
            Height::Refuel => NetworkVersion::V3,
            Height::Assembly => NetworkVersion::V4,
            Height::Tape => NetworkVersion::V5,
            Height::Liftoff => NetworkVersion::V5,
            Height::Kumquat => NetworkVersion::V6,
            Height::Calico => NetworkVersion::V7,
            Height::Persian => NetworkVersion::V8,
            Height::Orange => NetworkVersion::V9,
            Height::Claus => NetworkVersion::V9,
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
            Height::WatermelonFix => NetworkVersion::V21,
            Height::WatermelonFix2 => NetworkVersion::V21,
            Height::Dragon => NetworkVersion::V22,
            Height::DragonFix => NetworkVersion::V22,
            Height::Phoenix => NetworkVersion::V22,
            Height::Waffle => NetworkVersion::V23,
            Height::TukTuk => NetworkVersion::V24,
            Height::Teep => NetworkVersion::V25,
            Height::Tock => NetworkVersion::V26,
            Height::TockFix => NetworkVersion::V26,
            Height::GoldenWeek => NetworkVersion::V27,
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct HeightInfo {
    pub epoch: ChainEpoch,
    pub bundle: Option<Cid>,
}

pub struct HeightInfoWithActorManifest<'a> {
    #[allow(dead_code)]
    pub height: Height,
    pub info: &'a HeightInfo,
    pub manifest_cid: Cid,
}

impl<'a> HeightInfoWithActorManifest<'a> {
    pub fn manifest(&self, store: &impl Blockstore) -> anyhow::Result<BuiltinActorManifest> {
        BuiltinActorManifest::load_manifest(store, &self.manifest_cid)
    }
}

#[derive(Clone)]
struct DrandPoint<'a> {
    pub height: ChainEpoch,
    pub config: &'a LazyLock<DrandConfig<'a>>,
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
    pub genesis_network: NetworkVersion,
    pub height_infos: HashMap<Height, HeightInfo>,
    #[cfg_attr(test, arbitrary(gen(|_g| Policy::default())))]
    pub policy: Policy,
    pub eth_chain_id: EthChainId,
    pub breeze_gas_tamping_duration: i64,
    // FIP0081 gradually comes into effect over this many epochs.
    pub fip0081_ramp_duration_epochs: u64,
    // See FIP-0100 and https://github.com/filecoin-project/lotus/pull/12938 for why this exists
    pub upgrade_teep_initial_fil_reserved: Option<TokenAmount>,
    pub f3_enabled: bool,
    // F3Consensus set whether F3 should checkpoint tipsets finalized by F3. This flag has no effect if F3 is not enabled.
    pub f3_consensus: bool,
    pub f3_bootstrap_epoch: i64,
    pub f3_initial_power_table: Option<Cid>,
    pub enable_indexer: bool,
    pub enable_receipt_event_caching: bool,
    pub default_max_fee: TokenAmount,
}

impl ChainConfig {
    pub fn mainnet() -> Self {
        use mainnet::*;
        Self {
            network: NetworkChain::Mainnet,
            genesis_cid: Some(GENESIS_CID.to_string()),
            bootstrap_peers: DEFAULT_BOOTSTRAP.clone(),
            block_delay_secs: env_or_default(
                ENV_FOREST_BLOCK_DELAY_SECS,
                EPOCH_DURATION_SECONDS as u32,
            ),
            propagation_delay_secs: env_or_default(ENV_FOREST_PROPAGATION_DELAY_SECS, 10),
            genesis_network: GENESIS_NETWORK_VERSION,
            height_infos: HEIGHT_INFOS.clone(),
            policy: make_mainnet_policy!(v13).into(),
            eth_chain_id: ETH_CHAIN_ID,
            breeze_gas_tamping_duration: BREEZE_GAS_TAMPING_DURATION,
            // 1 year on mainnet
            fip0081_ramp_duration_epochs: 365 * EPOCHS_IN_DAY as u64,
            upgrade_teep_initial_fil_reserved: None,
            f3_enabled: true,
            f3_consensus: true,
            // April 29 at 10:00 UTC
            f3_bootstrap_epoch: 4920480,
            f3_initial_power_table: Some(
                "bafy2bzacecklgxd2eksmodvhgurqvorkg3wamgqkrunir3al2gchv2cikgmbu"
                    .parse()
                    .expect("invalid f3_initial_power_table"),
            ),
            enable_indexer: false,
            enable_receipt_event_caching: true,
            default_max_fee: TokenAmount::zero(),
        }
    }

    pub fn calibnet() -> Self {
        use calibnet::*;
        Self {
            network: NetworkChain::Calibnet,
            genesis_cid: Some(GENESIS_CID.to_string()),
            bootstrap_peers: DEFAULT_BOOTSTRAP.clone(),
            block_delay_secs: env_or_default(
                ENV_FOREST_BLOCK_DELAY_SECS,
                EPOCH_DURATION_SECONDS as u32,
            ),
            propagation_delay_secs: env_or_default(ENV_FOREST_PROPAGATION_DELAY_SECS, 10),
            genesis_network: GENESIS_NETWORK_VERSION,
            height_infos: HEIGHT_INFOS.clone(),
            policy: make_calibnet_policy!(v13).into(),
            eth_chain_id: ETH_CHAIN_ID,
            breeze_gas_tamping_duration: BREEZE_GAS_TAMPING_DURATION,
            // 3 days on calibnet
            fip0081_ramp_duration_epochs: 3 * EPOCHS_IN_DAY as u64,
            // FIP-0100: 300M -> 1.2B FIL
            upgrade_teep_initial_fil_reserved: Some(TokenAmount::from_whole(1_200_000_000)),
            // Enable after `f3_initial_power_table` is determined and set to avoid GC hell
            // (state tree of epoch 3_451_774 - 900 has to be present in the database if `f3_initial_power_table` is not set)
            f3_enabled: true,
            f3_consensus: true,
            // 2026-02-12T07:00:00Z
            f3_bootstrap_epoch: 3_451_774,
            f3_initial_power_table: Some(
                "bafy2bzacednijkh5dhb6jb7snxhhtjt7zuqaydlewoha3ordhy76dhgwtmptg"
                    .parse()
                    .expect("invalid f3_initial_power_table"),
            ),
            enable_indexer: false,
            enable_receipt_event_caching: true,
            default_max_fee: TokenAmount::zero(),
        }
    }

    pub fn devnet() -> Self {
        use devnet::*;
        Self {
            network: NetworkChain::Devnet("devnet".to_string()),
            genesis_cid: None,
            bootstrap_peers: Vec::new(),
            block_delay_secs: env_or_default(ENV_FOREST_BLOCK_DELAY_SECS, 4),
            propagation_delay_secs: env_or_default(ENV_FOREST_PROPAGATION_DELAY_SECS, 1),
            genesis_network: *GENESIS_NETWORK_VERSION,
            height_infos: HEIGHT_INFOS.clone(),
            policy: make_devnet_policy!(v13).into(),
            eth_chain_id: ETH_CHAIN_ID,
            breeze_gas_tamping_duration: BREEZE_GAS_TAMPING_DURATION,
            // Devnet ramp is 200 epochs in Lotus (subject to change).
            fip0081_ramp_duration_epochs: env_or_default(ENV_PLEDGE_RULE_RAMP, 200),
            // FIP-0100: 300M -> 1.4B FIL
            upgrade_teep_initial_fil_reserved: Some(TokenAmount::from_whole(1_400_000_000)),
            f3_enabled: false,
            f3_consensus: false,
            f3_bootstrap_epoch: -1,
            f3_initial_power_table: None,
            enable_indexer: false,
            enable_receipt_event_caching: true,
            default_max_fee: TokenAmount::zero(),
        }
    }

    pub fn butterflynet() -> Self {
        use butterflynet::*;
        Self {
            network: NetworkChain::Butterflynet,
            genesis_cid: Some(GENESIS_CID.to_string()),
            bootstrap_peers: DEFAULT_BOOTSTRAP.clone(),
            block_delay_secs: env_or_default(
                ENV_FOREST_BLOCK_DELAY_SECS,
                EPOCH_DURATION_SECONDS as u32,
            ),
            propagation_delay_secs: env_or_default(ENV_FOREST_PROPAGATION_DELAY_SECS, 6),
            genesis_network: GENESIS_NETWORK_VERSION,
            height_infos: HEIGHT_INFOS.clone(),
            policy: make_butterfly_policy!(v13).into(),
            eth_chain_id: ETH_CHAIN_ID,
            breeze_gas_tamping_duration: BREEZE_GAS_TAMPING_DURATION,
            // Butterflynet ramp is current set to 365 days in Lotus but this may change.
            fip0081_ramp_duration_epochs: env_or_default(
                ENV_PLEDGE_RULE_RAMP,
                365 * EPOCHS_IN_DAY as u64,
            ),
            // FIP-0100: 300M -> 1.6B FIL
            upgrade_teep_initial_fil_reserved: Some(TokenAmount::from_whole(1_600_000_000)),
            f3_enabled: true,
            f3_consensus: true,
            f3_bootstrap_epoch: 1000,
            f3_initial_power_table: None,
            enable_indexer: false,
            enable_receipt_event_caching: true,
            default_max_fee: TokenAmount::zero(),
        }
    }

    pub fn from_chain(network_chain: &NetworkChain) -> Self {
        match network_chain {
            NetworkChain::Mainnet => Self::mainnet(),
            NetworkChain::Calibnet => Self::calibnet(),
            NetworkChain::Butterflynet => Self::butterflynet(),
            NetworkChain::Devnet(name) => Self {
                network: NetworkChain::Devnet(name.clone()),
                ..Self::devnet()
            },
        }
    }

    fn network_height(&self, epoch: ChainEpoch) -> Option<Height> {
        self.height_infos
            .iter()
            .sorted_by_key(|(_, info)| info.epoch)
            .rev()
            .find(|(_, info)| epoch > info.epoch)
            .map(|(height, _)| *height)
    }

    /// Gets the latest network height prior to the given epoch that upgrades the actor bundle
    pub fn network_height_with_actor_bundle<'a>(
        &'a self,
        epoch: ChainEpoch,
    ) -> Option<HeightInfoWithActorManifest<'a>> {
        if let Some((height, info, manifest_cid)) = self
            .height_infos
            .iter()
            .sorted_by_key(|(_, info)| info.epoch)
            .rev()
            .filter_map(|(height, info)| info.bundle.map(|bundle| (*height, info, bundle)))
            .find(|(_, info, _)| epoch > info.epoch)
        {
            Some(HeightInfoWithActorManifest {
                height,
                info,
                manifest_cid,
            })
        } else {
            None
        }
    }

    /// Returns the network version at the given epoch.
    /// If the epoch is before the first upgrade, the genesis network version is returned.
    pub fn network_version(&self, epoch: ChainEpoch) -> NetworkVersion {
        self.network_height(epoch)
            .map(NetworkVersion::from)
            .unwrap_or(self.genesis_network_version())
            .max(self.genesis_network)
    }

    /// Returns the network version revision at the given epoch for distinguishing network upgrades
    /// that do not bump the network version.
    pub fn network_version_revision(&self, epoch: ChainEpoch) -> i64 {
        if let Some(height) = self.network_height(epoch) {
            let nv = NetworkVersion::from(height);
            if let Some(rev0_height) = Height::iter().find(|h| NetworkVersion::from(*h) == nv) {
                return (height as i64) - (rev0_height as i64);
            }
        }
        0
    }

    pub fn get_beacon_schedule(&self, genesis_ts: u64) -> BeaconSchedule {
        let ds_iter = match self.network {
            NetworkChain::Mainnet => mainnet::DRAND_SCHEDULE.iter(),
            NetworkChain::Calibnet => calibnet::DRAND_SCHEDULE.iter(),
            NetworkChain::Butterflynet => butterflynet::DRAND_SCHEDULE.iter(),
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
        self.height_infos
            .iter()
            .sorted_by_key(|(_, info)| info.epoch)
            .rev()
            .find_map(|(infos_height, info)| {
                if *infos_height == height {
                    Some(info.epoch)
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }

    pub async fn genesis_bytes<DB: SettingsStore>(
        &self,
        db: &DB,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(match self.network {
            NetworkChain::Mainnet => Some(mainnet::DEFAULT_GENESIS.to_vec()),
            NetworkChain::Calibnet => Some(calibnet::DEFAULT_GENESIS.to_vec()),
            // Butterflynet genesis is not hardcoded in the binary, for size reasons.
            NetworkChain::Butterflynet => Some(butterflynet::fetch_genesis(db).await?),
            NetworkChain::Devnet(_) => None,
        })
    }

    pub fn is_testnet(&self) -> bool {
        self.network.is_testnet()
    }

    pub fn is_devnet(&self) -> bool {
        self.network.is_devnet()
    }

    pub fn genesis_network_version(&self) -> NetworkVersion {
        self.genesis_network
    }

    pub fn initial_fil_reserved(&self, network_version: NetworkVersion) -> &TokenAmount {
        match &self.upgrade_teep_initial_fil_reserved {
            Some(fil) if network_version >= NetworkVersion::V25 => fil,
            _ => &INITIAL_FIL_RESERVED,
        }
    }

    pub fn initial_fil_reserved_at_height(&self, height: i64) -> &TokenAmount {
        let network_version = self.network_version(height);
        self.initial_fil_reserved(network_version)
    }
}

impl Default for ChainConfig {
    fn default() -> Self {
        ChainConfig::mainnet()
    }
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

#[allow(dead_code)]
fn get_upgrade_epoch_by_height<'a>(
    mut height_infos: impl Iterator<Item = &'a (Height, HeightInfo)>,
    height: Height,
) -> Option<ChainEpoch> {
    height_infos.find_map(|(infos_height, info)| {
        if *infos_height == height {
            Some(info.epoch)
        } else {
            None
        }
    })
}

fn get_upgrade_height_from_env(env_var_key: &str) -> Option<ChainEpoch> {
    if let Ok(value) = std::env::var(env_var_key) {
        if let Ok(epoch) = value.parse() {
            return Some(epoch);
        } else {
            warn!("Failed to parse {env_var_key}={value}, value should be an integer");
        }
    }
    None
}

#[macro_export]
macro_rules! make_height {
    ($id:ident,$epoch:expr) => {
        (
            Height::$id,
            HeightInfo {
                epoch: $epoch,
                bundle: None,
            },
        )
    };
    ($id:ident,$epoch:expr,$bundle:expr) => {
        (
            Height::$id,
            HeightInfo {
                epoch: $epoch,
                bundle: Some(Cid::try_from($bundle).unwrap()),
            },
        )
    };
}

// The formula matches lotus
// ```go
// sinceGenesis := build.Clock.Now().Sub(genesisTime)
// expectedHeight := int64(sinceGenesis.Seconds()) / int64(build.BlockDelaySecs)
// ```
// See <https://github.com/filecoin-project/lotus/blob/b27c861485695d3f5bb92bcb281abc95f4d90fb6/chain/sync.go#L180>
pub fn calculate_expected_epoch(
    now_timestamp: u64,
    genesis_timestamp: u64,
    block_delay_secs: u32,
) -> i64 {
    (now_timestamp.saturating_sub(genesis_timestamp) / block_delay_secs as u64) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heights_are_present(height_infos: &HashMap<Height, HeightInfo>) {
        /// These are required heights that need to be defined for all networks, for, e.g., conformance
        /// with `Filecoin.StateGetNetworkParams` RPC method.
        const REQUIRED_HEIGHTS: [Height; 30] = [
            Height::Breeze,
            Height::Smoke,
            Height::Ignition,
            Height::Refuel,
            Height::Assembly,
            Height::Tape,
            Height::Liftoff,
            Height::Kumquat,
            Height::Calico,
            Height::Persian,
            Height::Orange,
            Height::Claus,
            Height::Trust,
            Height::Norwegian,
            Height::Turbo,
            Height::Hyperdrive,
            Height::Chocolate,
            Height::OhSnap,
            Height::Skyr,
            Height::Shark,
            Height::Hygge,
            Height::Lightning,
            Height::Thunder,
            Height::Watermelon,
            Height::Dragon,
            Height::Phoenix,
            Height::Waffle,
            Height::TukTuk,
            Height::Teep,
            Height::GoldenWeek,
        ];

        for height in &REQUIRED_HEIGHTS {
            assert!(height_infos.get(height).is_some());
        }
    }

    #[test]
    fn test_mainnet_heights() {
        heights_are_present(&mainnet::HEIGHT_INFOS);
    }

    #[test]
    fn test_calibnet_heights() {
        heights_are_present(&calibnet::HEIGHT_INFOS);
    }

    #[test]
    fn test_devnet_heights() {
        heights_are_present(&devnet::HEIGHT_INFOS);
    }

    #[test]
    fn test_butterflynet_heights() {
        heights_are_present(&butterflynet::HEIGHT_INFOS);
    }

    #[test]
    fn test_get_upgrade_height_no_env_var() {
        let epoch = get_upgrade_height_from_env("FOREST_TEST_VAR_1");
        assert_eq!(epoch, None);
    }

    #[test]
    fn test_get_upgrade_height_valid_env_var() {
        unsafe { std::env::set_var("FOREST_TEST_VAR_2", "10") };
        let epoch = get_upgrade_height_from_env("FOREST_TEST_VAR_2");
        assert_eq!(epoch, Some(10));
    }

    #[test]
    fn test_get_upgrade_height_invalid_env_var() {
        unsafe { std::env::set_var("FOREST_TEST_VAR_3", "foo") };
        let epoch = get_upgrade_height_from_env("FOREST_TEST_VAR_3");
        assert_eq!(epoch, None);
    }

    #[test]
    fn test_calculate_expected_epoch() {
        // now, genesis, block_delay
        assert_eq!(0, calculate_expected_epoch(0, 0, 1));
        assert_eq!(5, calculate_expected_epoch(5, 0, 1));

        let mainnet_genesis = 1598306400;
        let mainnet_block_delay = 30;

        assert_eq!(
            0,
            calculate_expected_epoch(mainnet_genesis, mainnet_genesis, mainnet_block_delay)
        );

        assert_eq!(
            0,
            calculate_expected_epoch(
                mainnet_genesis + mainnet_block_delay as u64 - 1,
                mainnet_genesis,
                mainnet_block_delay
            )
        );

        assert_eq!(
            1,
            calculate_expected_epoch(
                mainnet_genesis + mainnet_block_delay as u64,
                mainnet_genesis,
                mainnet_block_delay
            )
        );
    }

    #[test]
    fn network_chain_display() {
        assert_eq!(
            NetworkChain::Mainnet.to_string(),
            mainnet::NETWORK_COMMON_NAME
        );
        assert_eq!(
            NetworkChain::Calibnet.to_string(),
            calibnet::NETWORK_COMMON_NAME
        );
        assert_eq!(
            NetworkChain::Butterflynet.to_string(),
            butterflynet::NETWORK_COMMON_NAME
        );
        assert_eq!(
            NetworkChain::Devnet("dummydevnet".into()).to_string(),
            "dummydevnet"
        );
    }

    #[test]
    fn chain_config() {
        ChainConfig::mainnet();
        ChainConfig::calibnet();
        ChainConfig::devnet();
        ChainConfig::butterflynet();
    }

    #[test]
    fn network_version() {
        let cfg = ChainConfig::calibnet();
        assert_eq!(cfg.network_version(1_013_134 - 1), NetworkVersion::V20);
        assert_eq!(cfg.network_version(1_013_134), NetworkVersion::V20);
        assert_eq!(cfg.network_version(1_013_134 + 1), NetworkVersion::V21);
        assert_eq!(cfg.network_version_revision(1_013_134 + 1), 0);
        assert_eq!(cfg.network_version(1_070_494), NetworkVersion::V21);
        assert_eq!(cfg.network_version_revision(1_070_494), 0);
        assert_eq!(cfg.network_version(1_070_494 + 1), NetworkVersion::V21);
        assert_eq!(cfg.network_version_revision(1_070_494 + 1), 1);
    }

    #[test]
    fn test_network_height_with_actor_bundle() {
        let cfg = ChainConfig::mainnet();
        let info = cfg.network_height_with_actor_bundle(5_348_280 + 1).unwrap();
        assert_eq!(info.height, Height::GoldenWeek);
        let info = cfg.network_height_with_actor_bundle(5_348_280).unwrap();
        // No actor bundle for Tock, so it should be Teep
        assert_eq!(info.height, Height::Teep);
        let info = cfg.network_height_with_actor_bundle(5_348_280 - 1).unwrap();
        assert_eq!(info.height, Height::Teep);
        assert!(cfg.network_height_with_actor_bundle(1).is_none());
        assert!(cfg.network_height_with_actor_bundle(0).is_none());
    }
}
