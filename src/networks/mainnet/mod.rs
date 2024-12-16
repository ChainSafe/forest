// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    eth::EthChainId,
    make_height,
    shim::{clock::ChainEpoch, version::NetworkVersion},
};
use ahash::HashMap;
use cid::Cid;
use libp2p::Multiaddr;
use once_cell::sync::Lazy;
use std::str::FromStr;

use super::{
    actors_bundle::ACTOR_BUNDLES_METADATA,
    drand::{DRAND_INCENTINET, DRAND_MAINNET, DRAND_QUICKNET},
    parse_bootstrap_peers, DrandPoint, Height, HeightInfo, NetworkChain,
};

const SMOKE_HEIGHT: ChainEpoch = 51000;

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");
/// Genesis CID
pub static GENESIS_CID: Lazy<Cid> = Lazy::new(|| {
    Cid::from_str("bafy2bzacecnamqgqmifpluoeldx7zzglxcljo6oja4vrmtj7432rphldpdmm2").unwrap()
});
pub const GENESIS_NETWORK_VERSION: NetworkVersion = NetworkVersion::V0;

pub static DEFAULT_BOOTSTRAP: Lazy<Vec<Multiaddr>> =
    Lazy::new(|| parse_bootstrap_peers(include_str!("../../../build/bootstrap/mainnet")));

// The rollover period is the duration between nv19 and nv20 which both old
// proofs (v1) and the new proofs (v1_1) proofs will be accepted by the
// network.
const LIGHTNING_ROLLOVER_PERIOD: i64 = 2880 * 21;

// https://github.com/ethereum-lists/chains/blob/4731f6713c6fc2bf2ae727388642954a6545b3a9/_data/chains/eip155-314.json
pub const ETH_CHAIN_ID: EthChainId = 314;

pub const BREEZE_GAS_TAMPING_DURATION: i64 = 120;

/// Height epochs.
pub static HEIGHT_INFOS: Lazy<HashMap<Height, HeightInfo>> = Lazy::new(|| {
    HashMap::from_iter([
        make_height!(Breeze, 41_280),
        make_height!(Smoke, SMOKE_HEIGHT),
        make_height!(Ignition, 94_000),
        make_height!(Refuel, 130_800),
        make_height!(Assembly, 138_720),
        make_height!(Tape, 140_760),
        make_height!(Liftoff, 148_888),
        make_height!(Kumquat, 170_000),
        make_height!(Calico, 265_200),
        make_height!(Persian, 272_400),
        make_height!(Orange, 336_458),
        make_height!(Claus, 343_200),
        make_height!(Trust, 550_321),
        make_height!(Norwegian, 665_280),
        make_height!(Turbo, 712_320),
        make_height!(Hyperdrive, 892_800),
        make_height!(Chocolate, 1_231_620),
        make_height!(OhSnap, 1_594_680),
        make_height!(Skyr, 1_960_320),
        make_height!(Shark, 2_383_680, get_bundle_cid("v9.0.3")),
        make_height!(Hygge, 2_683_348, get_bundle_cid("v10.0.0")),
        make_height!(Lightning, 2_809_800, get_bundle_cid("v11.0.0")),
        make_height!(Thunder, 2_809_800 + LIGHTNING_ROLLOVER_PERIOD),
        make_height!(Watermelon, 3_469_380, get_bundle_cid("v12.0.0")),
        // Thu Apr 24 02:00:00 PM UTC 2024
        make_height!(Dragon, 3_855_360, get_bundle_cid("v13.0.0")),
        make_height!(Phoenix, 3_855_480),
        // Tue  6 Aug 12:00:00 UTC 2024
        make_height!(Waffle, 4_154_640, get_bundle_cid("v14.0.0")),
        // Wed 20 Nov 23:00:00 UTC 2024
        make_height!(TukTuk, 4_461_240, get_bundle_cid("v15.0.0")),
        // TODO(forest): https://github.com/ChainSafe/forest/issues/5041
        make_height!(Teep, i64::MAX, get_bundle_cid("v15.0.0")),
    ])
});

fn get_bundle_cid(version: &str) -> Cid {
    ACTOR_BUNDLES_METADATA
        .get(&(NetworkChain::Mainnet, version.into()))
        .expect("bundle must be defined")
        .bundle_cid
}

pub(super) static DRAND_SCHEDULE: Lazy<[DrandPoint<'static>; 3]> = Lazy::new(|| {
    [
        DrandPoint {
            height: 0,
            config: &DRAND_INCENTINET,
        },
        DrandPoint {
            height: SMOKE_HEIGHT,
            config: &DRAND_MAINNET,
        },
        DrandPoint {
            height: HEIGHT_INFOS
                .get(&Height::Phoenix)
                .expect("Phoenix height must be defined")
                .epoch,
            config: &DRAND_QUICKNET,
        },
    ]
});

/// Creates a new mainnet policy with the given version.
#[macro_export]
macro_rules! make_mainnet_policy {
    ($version:tt) => {
        fil_actors_shared::$version::runtime::Policy::default()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_boostrap_list_not_empty() {
        assert!(!DEFAULT_BOOTSTRAP.is_empty());
    }
}
