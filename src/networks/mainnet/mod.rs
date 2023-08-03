// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::clock::ChainEpoch;
use cid::Cid;
use libp2p::Multiaddr;
use once_cell::sync::Lazy;
use std::str::FromStr;

use super::{
    drand::{DRAND_INCENTINET, DRAND_MAINNET},
    parse_bootstrap_peers, DrandPoint, Height, HeightInfo,
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
pub static HEIGHT_INFOS: Lazy<[HeightInfo; 21]> = Lazy::new(|| {
    [
        HeightInfo {
            height: Height::Breeze,
            epoch: 41_280,
            bundle: None,
        },
        HeightInfo {
            height: Height::Smoke,
            epoch: SMOKE_HEIGHT,
            bundle: None,
        },
        HeightInfo {
            height: Height::Ignition,
            epoch: 94_000,
            bundle: None,
        },
        HeightInfo {
            height: Height::ActorsV2,
            epoch: 138_720,
            bundle: None,
        },
        HeightInfo {
            height: Height::Tape,
            epoch: 140_760,
            bundle: None,
        },
        HeightInfo {
            height: Height::Liftoff,
            epoch: 148_888,
            bundle: None,
        },
        HeightInfo {
            height: Height::Kumquat,
            epoch: 170_000,
            bundle: None,
        },
        HeightInfo {
            height: Height::Calico,
            epoch: 265_200,
            bundle: None,
        },
        HeightInfo {
            height: Height::Persian,
            epoch: 272_400,
            bundle: None,
        },
        HeightInfo {
            height: Height::Orange,
            epoch: 336_458,
            bundle: None,
        },
        HeightInfo {
            height: Height::Trust,
            epoch: 550_321,
            bundle: None,
        },
        HeightInfo {
            height: Height::Norwegian,
            epoch: 665_280,
            bundle: None,
        },
        HeightInfo {
            height: Height::Turbo,
            epoch: 712_320,
            bundle: None,
        },
        HeightInfo {
            height: Height::Hyperdrive,
            epoch: 892_800,
            bundle: None,
        },
        HeightInfo {
            height: Height::Chocolate,
            epoch: 1_231_620,
            bundle: None,
        },
        HeightInfo {
            height: Height::OhSnap,
            epoch: 1_594_680,
            bundle: None,
        },
        HeightInfo {
            height: Height::Skyr,
            epoch: 1_960_320,
            bundle: None,
        },
        HeightInfo {
            height: Height::Shark,
            epoch: 2_383_680,
            bundle: Some(
                Cid::try_from("bafy2bzaceb6j6666h36xnhksu3ww4kxb6e25niayfgkdnifaqi6m6ooc66i6i")
                    .unwrap(),
            ),
        },
        HeightInfo {
            height: Height::Hygge,
            epoch: 2_683_348,
            bundle: Some(
                Cid::try_from("bafy2bzacecsuyf7mmvrhkx2evng5gnz5canlnz2fdlzu2lvcgptiq2pzuovos")
                    .unwrap(),
            ),
        },
        HeightInfo {
            height: Height::Lightning,
            epoch: 2_809_800,
            bundle: Some(
                Cid::try_from("bafy2bzacecnhaiwcrpyjvzl4uv4q3jzoif26okl3m66q3cijp3dfwlcxwztwo")
                    .unwrap(),
            ),
        },
        HeightInfo {
            height: Height::Thunder,
            epoch: 2_809_800 + LIGHTNING_ROLLOVER_PERIOD,
            bundle: None,
        },
    ]
});

pub(super) static DRAND_SCHEDULE: [DrandPoint<'static>; 2] = [
    DrandPoint {
        height: 0,
        config: &DRAND_INCENTINET,
    },
    DrandPoint {
        height: SMOKE_HEIGHT,
        config: &DRAND_MAINNET,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_boostrap_list_not_empty() {
        assert!(!DEFAULT_BOOTSTRAP.is_empty());
    }
}
