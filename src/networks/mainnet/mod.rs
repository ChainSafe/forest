// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::clock::ChainEpoch;
use ahash::HashMap;
use cid::Cid;
use libp2p::Multiaddr;
use once_cell::sync::Lazy;
use std::str::FromStr;

use super::{
    drand::{DRAND_INCENTINET, DRAND_MAINNET, DRAND_QUICKNET},
    get_upgrade_height_from_env, parse_bootstrap_peers, DrandPoint, Height, HeightInfo,
};

const SMOKE_HEIGHT: ChainEpoch = 51000;

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");
/// Genesis CID
pub static GENESIS_CID: Lazy<Cid> = Lazy::new(|| {
    Cid::from_str("bafy2bzacecnamqgqmifpluoeldx7zzglxcljo6oja4vrmtj7432rphldpdmm2").unwrap()
});

pub static DEFAULT_BOOTSTRAP: Lazy<Vec<Multiaddr>> =
    Lazy::new(|| parse_bootstrap_peers(include_str!("../../../build/bootstrap/mainnet")));

// The rollover period is the duration between nv19 and nv20 which both old
// proofs (v1) and the new proofs (v1_1) proofs will be accepted by the
// network.
const LIGHTNING_ROLLOVER_PERIOD: i64 = 2880 * 21;

// https://github.com/ethereum-lists/chains/blob/4731f6713c6fc2bf2ae727388642954a6545b3a9/_data/chains/eip155-314.json
pub const ETH_CHAIN_ID: u64 = 314;

/// Height epochs.
pub static HEIGHT_INFOS: Lazy<HashMap<Height, HeightInfo>> = Lazy::new(|| {
    HashMap::from_iter([
        (
            Height::Breeze,
            HeightInfo {
                epoch: 41_280,
                bundle: None,
            },
        ),
        (
            Height::Smoke,
            HeightInfo {
                epoch: SMOKE_HEIGHT,
                bundle: None,
            },
        ),
        (
            Height::Ignition,
            HeightInfo {
                epoch: 94_000,
                bundle: None,
            },
        ),
        (
            Height::ActorsV2,
            HeightInfo {
                epoch: 138_720,
                bundle: None,
            },
        ),
        (
            Height::Tape,
            HeightInfo {
                epoch: 140_760,
                bundle: None,
            },
        ),
        (
            Height::Liftoff,
            HeightInfo {
                epoch: 148_888,
                bundle: None,
            },
        ),
        (
            Height::Kumquat,
            HeightInfo {
                epoch: 170_000,
                bundle: None,
            },
        ),
        (
            Height::Calico,
            HeightInfo {
                epoch: 265_200,
                bundle: None,
            },
        ),
        (
            Height::Persian,
            HeightInfo {
                epoch: 272_400,
                bundle: None,
            },
        ),
        (
            Height::Orange,
            HeightInfo {
                epoch: 336_458,
                bundle: None,
            },
        ),
        (
            Height::Trust,
            HeightInfo {
                epoch: 550_321,
                bundle: None,
            },
        ),
        (
            Height::Norwegian,
            HeightInfo {
                epoch: 665_280,
                bundle: None,
            },
        ),
        (
            Height::Turbo,
            HeightInfo {
                epoch: 712_320,
                bundle: None,
            },
        ),
        (
            Height::Hyperdrive,
            HeightInfo {
                epoch: 892_800,
                bundle: None,
            },
        ),
        (
            Height::Chocolate,
            HeightInfo {
                epoch: 1_231_620,
                bundle: None,
            },
        ),
        (
            Height::OhSnap,
            HeightInfo {
                epoch: 1_594_680,
                bundle: None,
            },
        ),
        (
            Height::Skyr,
            HeightInfo {
                epoch: 1_960_320,
                bundle: None,
            },
        ),
        (
            Height::Shark,
            HeightInfo {
                epoch: 2_383_680,
                bundle: Some(
                    Cid::try_from("bafy2bzaceb6j6666h36xnhksu3ww4kxb6e25niayfgkdnifaqi6m6ooc66i6i")
                        .unwrap(),
                ),
            },
        ),
        (
            Height::Hygge,
            HeightInfo {
                epoch: 2_683_348,
                bundle: Some(
                    Cid::try_from("bafy2bzacecsuyf7mmvrhkx2evng5gnz5canlnz2fdlzu2lvcgptiq2pzuovos")
                        .unwrap(),
                ),
            },
        ),
        (
            Height::Lightning,
            HeightInfo {
                epoch: 2_809_800,
                bundle: Some(
                    Cid::try_from("bafy2bzacecnhaiwcrpyjvzl4uv4q3jzoif26okl3m66q3cijp3dfwlcxwztwo")
                        .unwrap(),
                ),
            },
        ),
        (
            Height::Thunder,
            HeightInfo {
                epoch: 2_809_800 + LIGHTNING_ROLLOVER_PERIOD,
                bundle: None,
            },
        ),
        (
            Height::Watermelon,
            HeightInfo {
                epoch: 3_469_380,
                bundle: Some(
                    Cid::try_from("bafy2bzaceapkgfggvxyllnmuogtwasmsv5qi2qzhc2aybockd6kag2g5lzaio")
                        .unwrap(),
                ),
            },
        ),
        (
            Height::Dragon,
            HeightInfo {
                epoch: get_upgrade_height_from_env("FOREST_DRAGON_HEIGHT").unwrap_or(i64::MAX),
                bundle: Some(
                    Cid::try_from("bafy2bzacea6f5icdp6t6fs5sexjxmo3q5d2qu4g4ghq6s5eaob6svnmhvltmw")
                        .unwrap(),
                ),
            },
        ),
    ])
});

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
            // height is TBD
            height: i64::MAX,
            config: &DRAND_QUICKNET,
        },
    ]
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_boostrap_list_not_empty() {
        assert!(!DEFAULT_BOOTSTRAP.is_empty());
    }
}
