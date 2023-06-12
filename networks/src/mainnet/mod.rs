// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_shim::clock::ChainEpoch;
use lazy_static::lazy_static;
use url::Url;

use super::{
    drand::{DRAND_INCENTINET, DRAND_MAINNET},
    DrandPoint, Height, HeightInfo,
};
use crate::ActorBundleInfo;

const SMOKE_HEIGHT: ChainEpoch = ChainEpoch(51000);

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");
/// Genesis CID
pub const GENESIS_CID: &str = "bafy2bzacecnamqgqmifpluoeldx7zzglxcljo6oja4vrmtj7432rphldpdmm2";

/// Default bootstrap peer ids.
pub const DEFAULT_BOOTSTRAP: &[&str] =
    &const_str::split!(include_str!("../../../build/bootstrap/mainnet"), "\n");

// The rollover period is the duration between nv19 and nv20 which both old
// proofs (v1) and the new proofs (v1_1) proofs will be accepted by the
// network.
const LIGHTNING_ROLLOVER_PERIOD: i64 = 2880 * 21;

lazy_static! {
/// Height epochs.
pub static ref HEIGHT_INFOS: [HeightInfo; 21] = [
    HeightInfo {
        height: Height::Breeze,
        epoch: ChainEpoch(41_280),
        bundle: None,
    },
    HeightInfo {
        height: Height::Smoke,
        epoch: SMOKE_HEIGHT,
        bundle: None,
    },
    HeightInfo {
        height: Height::Ignition,
        epoch: ChainEpoch(94_000),
        bundle: None,
    },
    HeightInfo {
        height: Height::ActorsV2,
        epoch: ChainEpoch(138_720),
        bundle: None,
    },
    HeightInfo {
        height: Height::Tape,
        epoch: ChainEpoch(140_760),
        bundle: None,
    },
    HeightInfo {
        height: Height::Liftoff,
        epoch: ChainEpoch(148_888),
        bundle: None,
    },
    HeightInfo {
        height: Height::Kumquat,
        epoch: ChainEpoch(170_000),
        bundle: None,
    },
    HeightInfo {
        height: Height::Calico,
        epoch: ChainEpoch(265_200),
        bundle: None,
    },
    HeightInfo {
        height: Height::Persian,
        epoch: ChainEpoch(272_400),
        bundle: None,
    },
    HeightInfo {
        height: Height::Orange,
        epoch: ChainEpoch(336_458),
        bundle: None,
    },
    HeightInfo {
        height: Height::Trust,
        epoch: ChainEpoch(550_321),
        bundle: None,
    },
    HeightInfo {
        height: Height::Norwegian,
        epoch: ChainEpoch(665_280),
        bundle: None,
    },
    HeightInfo {
        height: Height::Turbo,
        epoch: ChainEpoch(712_320),
        bundle: None,
    },
    HeightInfo {
        height: Height::Hyperdrive,
        epoch: ChainEpoch(892_800),
        bundle: None,
    },
    HeightInfo {
        height: Height::Chocolate,
        epoch: ChainEpoch(1_231_620),
        bundle: None,
    },
    HeightInfo {
        height: Height::OhSnap,
        epoch: ChainEpoch(1_594_680),
        bundle: None,
    },
    HeightInfo {
        height: Height::Skyr,
        epoch: ChainEpoch(1_960_320),
        bundle: None,
    },
    HeightInfo {
        height: Height::Shark,
        epoch: ChainEpoch(2_383_680),
        bundle: None,
    },
    HeightInfo {
        height: Height::Hygge,
        epoch: ChainEpoch(2_683_348),
        bundle: Some(ActorBundleInfo {
            manifest: Cid::try_from("bafy2bzacecsuyf7mmvrhkx2evng5gnz5canlnz2fdlzu2lvcgptiq2pzuovos").unwrap(),
            url: Url::parse("https://github.com/filecoin-project/builtin-actors/releases/download/v10.0.0/builtin-actors-mainnet.car").unwrap()
    }),
    },
    HeightInfo {
        height: Height::Lightning,
        epoch: ChainEpoch(2_809_800),
        bundle: Some(ActorBundleInfo {
            manifest: Cid::try_from("bafy2bzacecnhaiwcrpyjvzl4uv4q3jzoif26okl3m66q3cijp3dfwlcxwztwo").unwrap(),
            url: Url::parse("https://github.com/filecoin-project/builtin-actors/releases/download/v11.0.0/builtin-actors-mainnet.car").unwrap()
    }),
    },
    HeightInfo {
        height: Height::Thunder,
        epoch: ChainEpoch(2_809_800 + LIGHTNING_ROLLOVER_PERIOD),
        bundle: None,
    },
];
}

pub(super) static DRAND_SCHEDULE: [DrandPoint<'static>; 2] = [
    DrandPoint {
        height: ChainEpoch(0),
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
        DEFAULT_BOOTSTRAP.iter().for_each(|addr| {
            assert!(addr.parse::<multiaddr::Multiaddr>().is_ok());
        });
    }
}
