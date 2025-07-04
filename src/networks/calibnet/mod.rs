// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use cid::Cid;
use libp2p::Multiaddr;
use once_cell::sync::Lazy;
use std::str::FromStr;

use crate::{
    eth::EthChainId,
    make_height,
    shim::{clock::EPOCHS_IN_DAY, version::NetworkVersion},
};

use super::{
    DrandPoint, Height, HeightInfo, NetworkChain,
    actors_bundle::ACTOR_BUNDLES_METADATA,
    drand::{DRAND_MAINNET, DRAND_QUICKNET},
    parse_bootstrap_peers,
};

/// Well-known network names.
pub const NETWORK_COMMON_NAME: &str = "calibnet";
/// Network name as defined in the genesis block. Refer to [`crate::networks::network_names`] for
/// additional information.
pub const NETWORK_GENESIS_NAME: &str = "calibrationnet";

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");
/// Genesis CID
pub static GENESIS_CID: Lazy<Cid> = Lazy::new(|| {
    Cid::from_str("bafy2bzacecyaggy24wol5ruvs6qm73gjibs2l2iyhcqmvi7r7a4ph7zx3yqd4").unwrap()
});
pub const GENESIS_NETWORK_VERSION: NetworkVersion = NetworkVersion::V0;

/// Default bootstrap peer ids.
pub static DEFAULT_BOOTSTRAP: Lazy<Vec<Multiaddr>> =
    Lazy::new(|| parse_bootstrap_peers(include_str!("../../../build/bootstrap/calibnet")));

const LIGHTNING_EPOCH: i64 = 489_094;

// The rollover period is the duration between nv19 and nv20 which both old
// proofs (v1) and the new proofs (v1_1) proofs will be accepted by the
// network.
const LIGHTNING_ROLLOVER_PERIOD: i64 = 3120;

// https://github.com/ethereum-lists/chains/blob/4731f6713c6fc2bf2ae727388642954a6545b3a9/_data/chains/eip155-314159.json
pub const ETH_CHAIN_ID: EthChainId = 314159;

pub const BREEZE_GAS_TAMPING_DURATION: i64 = 120;

/// Height epochs.
pub static HEIGHT_INFOS: Lazy<HashMap<Height, HeightInfo>> = Lazy::new(|| {
    HashMap::from_iter([
        make_height!(Breeze, -1),
        make_height!(Smoke, -2),
        make_height!(Ignition, -3),
        make_height!(Refuel, -4),
        make_height!(Assembly, 30),
        make_height!(Tape, 60),
        make_height!(Liftoff, -5),
        make_height!(Kumquat, 90),
        make_height!(Calico, 120),
        make_height!(Persian, 240),
        make_height!(Claus, 270),
        make_height!(Orange, 300),
        make_height!(Trust, 330),
        make_height!(Norwegian, 360),
        make_height!(Turbo, 390),
        make_height!(Hyperdrive, 420),
        make_height!(Chocolate, 450),
        make_height!(OhSnap, 480),
        make_height!(Skyr, 510, get_bundle_cid("8.0.0-rc.1")),
        make_height!(Shark, 16_800, get_bundle_cid("v9.0.3")),
        make_height!(Hygge, 322_354, get_bundle_cid("v10.0.0-rc.1")),
        make_height!(Lightning, LIGHTNING_EPOCH, get_bundle_cid("v11.0.0-rc2")),
        make_height!(Thunder, LIGHTNING_EPOCH + LIGHTNING_ROLLOVER_PERIOD),
        make_height!(Watermelon, 1_013_134, get_bundle_cid("v12.0.0-rc.1")),
        make_height!(WatermelonFix, 1_070_494, get_bundle_cid("v12.0.0-rc.2")),
        make_height!(WatermelonFix2, 1_108_174, get_bundle_cid("v12.0.0")),
        make_height!(Dragon, 1_427_974, get_bundle_cid("v13.0.0-rc.3")),
        make_height!(DragonFix, 1_493_854, get_bundle_cid("v13.0.0")),
        make_height!(Phoenix, 1_428_094),
        // 2024-07-11 12:00:00Z
        make_height!(Waffle, 1_779_094, get_bundle_cid("v14.0.0-rc.1")),
        // 2024-10-23T13:30:00Z
        make_height!(TukTuk, 2_078_794, get_bundle_cid("v15.0.0")),
        // 2025-03-26T23:00:00Z
        make_height!(Teep, 2_523_454, get_bundle_cid("v16.0.0-rc3")),
        // This epoch, 7 days after Teep is the completion of FIP-0100 where actors will start applying
        // the new daily fee to pre-Teep sectors being extended. This is 90 days on mainnet.
        make_height!(Tock, 2_523_454 + 7 * EPOCHS_IN_DAY),
        // Mon  7 Apr 23:00:00 UTC 2025
        make_height!(TockFix, 2_558_014, get_bundle_cid("v16.0.1")),
    ])
});

fn get_bundle_cid(version: &str) -> Cid {
    ACTOR_BUNDLES_METADATA
        .get(&(NetworkChain::Calibnet, version.into()))
        .expect("bundle must be defined")
        .bundle_cid
}

pub(super) static DRAND_SCHEDULE: Lazy<[DrandPoint<'static>; 2]> = Lazy::new(|| {
    [
        DrandPoint {
            height: 0,
            config: &DRAND_MAINNET,
        },
        DrandPoint {
            height: HEIGHT_INFOS.get(&Height::Phoenix).unwrap().epoch,
            config: &DRAND_QUICKNET,
        },
    ]
});

/// Creates a new calibnet policy with the given version.
#[macro_export]
macro_rules! make_calibnet_policy {
    ($version:tt) => {
        fil_actors_shared::$version::runtime::Policy {
            minimum_consensus_power: (32i64 << 30).into(),
            ..Default::default()
        }
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
