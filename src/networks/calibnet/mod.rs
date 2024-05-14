// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use cid::Cid;
use libp2p::Multiaddr;
use once_cell::sync::Lazy;
use std::str::FromStr;

use crate::{make_height, shim::version::NetworkVersion};

use super::{
    drand::{DRAND_MAINNET, DRAND_QUICKNET},
    get_upgrade_height_from_env, parse_bootstrap_peers, DrandPoint, Height, HeightInfo,
};

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
pub const ETH_CHAIN_ID: u64 = 314159;

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
        make_height!(Skyr, 510),
        make_height!(
            Shark,
            16_800,
            "bafy2bzacedbedgynklc4dgpyxippkxmba2mgtw7ecntoneclsvvl4klqwuyyy"
        ),
        make_height!(
            Hygge,
            322_354,
            "bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo"
        ),
        make_height!(
            Lightning,
            LIGHTNING_EPOCH,
            "bafy2bzacedhuowetjy2h4cxnijz2l64h4mzpk5m256oywp4evarpono3cjhco"
        ),
        make_height!(Thunder, LIGHTNING_EPOCH + LIGHTNING_ROLLOVER_PERIOD),
        make_height!(
            Watermelon,
            1_013_134,
            "bafy2bzacedrunxfqta5skb7q7x32lnp4efz2oq7fn226ffm7fu5iqs62jkmvs"
        ),
        make_height!(
            WatermelonFix,
            1_070_494,
            "bafy2bzacebl4w5ptfvuw6746w7ev562idkbf5ppq72e6zub22435ws2rukzru"
        ),
        make_height!(
            WatermelonFix2,
            1_108_174,
            "bafy2bzacednzb3pkrfnbfhmoqtb3bc6dgvxszpqklf3qcc7qzcage4ewzxsca"
        ),
        make_height!(
            Dragon,
            1_427_974,
            "bafy2bzacea4firkyvt2zzdwqjrws5pyeluaesh6uaid246tommayr4337xpmi"
        ),
        make_height!(
            DragonFix,
            1_493_854,
            "bafy2bzacect4ktyujrwp6mjlsitnpvuw2pbuppz6w52sfljyo4agjevzm75qs"
        ),
        make_height!(Phoenix, 1_428_094),
        make_height!(Aussie, 999999999999999),
    ])
});

pub(super) static DRAND_SCHEDULE: Lazy<[DrandPoint<'static>; 2]> = Lazy::new(|| {
    [
        DrandPoint {
            height: 0,
            config: &DRAND_MAINNET,
        },
        DrandPoint {
            height: get_upgrade_height_from_env("FOREST_DRAND_QUICKNET_HEIGHT")
                .unwrap_or(HEIGHT_INFOS.get(&Height::Phoenix).unwrap().epoch),
            config: &DRAND_QUICKNET,
        },
    ]
});

/// Creates a new mainnet policy with the given version.
#[macro_export]
macro_rules! make_calibnet_policy {
    ($version:tt) => {
        fil_actors_shared::$version::runtime::Policy {
            minimum_consensus_power: (32 << 30).into(),
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
