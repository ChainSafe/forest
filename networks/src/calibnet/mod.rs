// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_shim::clock::ChainEpoch;
use cid::Cid;
use lazy_static::lazy_static;
use url::Url;

use super::{drand::DRAND_MAINNET, DrandPoint, Height, HeightInfo};
use crate::ActorBundleInfo;

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");
/// Genesis CID
pub const GENESIS_CID: &str = "bafy2bzacecyaggy24wol5ruvs6qm73gjibs2l2iyhcqmvi7r7a4ph7zx3yqd4";

/// Default bootstrap peer ids.
pub const DEFAULT_BOOTSTRAP: &[&str] =
    &const_str::split!(include_str!("../../../build/bootstrap/calibnet"), "\n");

const LIGHTNING_EPOCH: i64 = 489_094;

// The rollover period is the duration between nv19 and nv20 which both old
// proofs (v1) and the new proofs (v1_1) proofs will be accepted by the
// network.
const LIGHTNING_ROLLOVER_PERIOD: i64 = 3120;

lazy_static! {
/// Height epochs.
pub static ref HEIGHT_INFOS: [HeightInfo; 21] = [
    HeightInfo {
        height: Height::Breeze,
        epoch: ChainEpoch(-1),
        bundle: None,
    },
    HeightInfo {
        height: Height::Smoke,
        epoch: ChainEpoch(-2),
        bundle: None,
    },
    HeightInfo {
        height: Height::Ignition,
        epoch: ChainEpoch(-3),
        bundle: None,
    },
    HeightInfo {
        height: Height::ActorsV2,
        epoch: ChainEpoch(30),
        bundle: None,
    },
    HeightInfo {
        height: Height::Tape,
        epoch: ChainEpoch(60),
        bundle: None,
    },
    HeightInfo {
        height: Height::Liftoff,
        epoch: ChainEpoch(-5),
        bundle: None,
    },
    HeightInfo {
        height: Height::Kumquat,
        epoch: ChainEpoch(90),
        bundle: None,
    },
    HeightInfo {
        height: Height::Calico,
        epoch: ChainEpoch(120),
        bundle: None,
    },
    HeightInfo {
        height: Height::Persian,
        epoch: ChainEpoch(130),
        bundle: None,
    },
    HeightInfo {
        height: Height::Orange,
        epoch: ChainEpoch(300),
        bundle: None,
    },
    HeightInfo {
        height: Height::Trust,
        epoch: ChainEpoch(330),
        bundle: None,
    },
    HeightInfo {
        height: Height::Norwegian,
        epoch: ChainEpoch(360),
        bundle: None,
    },
    HeightInfo {
        height: Height::Turbo,
        epoch: ChainEpoch(390),
        bundle: None,
    },
    HeightInfo {
        height: Height::Hyperdrive,
        epoch: ChainEpoch(420),
        bundle: None,
    },
    HeightInfo {
        height: Height::Chocolate,
        epoch: ChainEpoch(450),
        bundle: None,
    },
    HeightInfo {
        height: Height::OhSnap,
        epoch: ChainEpoch(480),
        bundle: None,
    },
    HeightInfo {
        height: Height::Skyr,
        epoch: ChainEpoch(510),
        bundle: None,
    },
    HeightInfo {
        height: Height::Shark,
        epoch: ChainEpoch(16_800),
        bundle: None,
    },
    HeightInfo {
        height: Height::Hygge,
        epoch: ChainEpoch(322_354),
        bundle: Some(ActorBundleInfo {
            manifest: Cid::try_from("bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo").unwrap(),
            url: Url::parse("https://github.com/filecoin-project/builtin-actors/releases/download/v10.0.0-rc.1/builtin-actors-calibrationnet.car").unwrap()
    })
    },
    HeightInfo {
        height: Height::Lightning,
        epoch: ChainEpoch(LIGHTNING_EPOCH),
        bundle: Some(ActorBundleInfo {
            manifest: Cid::try_from("bafy2bzacedhuowetjy2h4cxnijz2l64h4mzpk5m256oywp4evarpono3cjhco").unwrap(),
            url: Url::parse("https://github.com/filecoin-project/builtin-actors/releases/download/v11.0.0-rc2/builtin-actors-calibrationnet.car").unwrap()
    }),
    },
    HeightInfo {
        height: Height::Thunder,
        epoch: ChainEpoch(LIGHTNING_EPOCH + LIGHTNING_ROLLOVER_PERIOD),
        bundle: None,
    },
];
}

pub(super) static DRAND_SCHEDULE: [DrandPoint<'static>; 1] = [DrandPoint {
    height: ChainEpoch(0),
    config: &DRAND_MAINNET,
}];

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
