// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use lazy_static::lazy_static;
use url::Url;

use super::{drand::DRAND_MAINNET, DrandPoint, Height, HeightInfo};
use crate::ActorBundleInfo;

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");
/// Genesis CID
pub const GENESIS_CID: &str = "bafy2bzacecyaggy24wol5ruvs6qm73gjibs2l2iyhcqmvi7r7a4ph7zx3yqd4";

/// Bootstrap peer ids.
pub const DEFAULT_BOOTSTRAP: &[&str] = &[
    "/dns4/bootstrap-0.calibration.fildev.network/tcp/1347/p2p/12D3KooWCi2w8U4DDB9xqrejb5KYHaQv2iA2AJJ6uzG3iQxNLBMy",
    "/dns4/bootstrap-1.calibration.fildev.network/tcp/1347/p2p/12D3KooWDTayrBojBn9jWNNUih4nNQQBGJD7Zo3gQCKgBkUsS6dp",
    "/dns4/bootstrap-2.calibration.fildev.network/tcp/1347/p2p/12D3KooWNRxTHUn8bf7jz1KEUPMc2dMgGfa4f8ZJTsquVSn3vHCG",
    "/dns4/bootstrap-3.calibration.fildev.network/tcp/1347/p2p/12D3KooWFWUqE9jgXvcKHWieYs9nhyp6NF4ftwLGAHm4sCv73jjK",
];

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
        epoch: -1,
        bundle: None,
    },
    HeightInfo {
        height: Height::Smoke,
        epoch: -2,
        bundle: None,
    },
    HeightInfo {
        height: Height::Ignition,
        epoch: -3,
        bundle: None,
    },
    HeightInfo {
        height: Height::ActorsV2,
        epoch: 30,
        bundle: None,
    },
    HeightInfo {
        height: Height::Tape,
        epoch: 60,
        bundle: None,
    },
    HeightInfo {
        height: Height::Liftoff,
        epoch: -5,
        bundle: None,
    },
    HeightInfo {
        height: Height::Kumquat,
        epoch: 90,
        bundle: None,
    },
    HeightInfo {
        height: Height::Calico,
        epoch: 120,
        bundle: None,
    },
    HeightInfo {
        height: Height::Persian,
        epoch: 130,
        bundle: None,
    },
    HeightInfo {
        height: Height::Orange,
        epoch: 300,
        bundle: None,
    },
    HeightInfo {
        height: Height::Trust,
        epoch: 330,
        bundle: None,
    },
    HeightInfo {
        height: Height::Norwegian,
        epoch: 360,
        bundle: None,
    },
    HeightInfo {
        height: Height::Turbo,
        epoch: 390,
        bundle: None,
    },
    HeightInfo {
        height: Height::Hyperdrive,
        epoch: 420,
        bundle: None,
    },
    HeightInfo {
        height: Height::Chocolate,
        epoch: 450,
        bundle: None,
    },
    HeightInfo {
        height: Height::OhSnap,
        epoch: 480,
        bundle: None,
    },
    HeightInfo {
        height: Height::Skyr,
        epoch: 510,
        bundle: None,
    },
    HeightInfo {
        height: Height::Shark,
        epoch: 16_800,
        bundle: None,
    },
    HeightInfo {
        height: Height::Hygge,
        epoch: 322_354,
        bundle: Some(ActorBundleInfo {
            manifest: Cid::try_from("bafy2bzaced25ta3j6ygs34roprilbtb3f6mxifyfnm7z7ndquaruxzdq3y7lo").unwrap(),
            url: Url::parse("https://github.com/filecoin-project/builtin-actors/releases/download/v10.0.0-rc.1/builtin-actors-calibrationnet.car").unwrap()
    })
    },
    HeightInfo {
        height: Height::Lightning,
        epoch: LIGHTNING_EPOCH,
        bundle: Some(ActorBundleInfo {
            manifest: Cid::try_from("bafy2bzacedhuowetjy2h4cxnijz2l64h4mzpk5m256oywp4evarpono3cjhco").unwrap(),
            url: Url::parse("https://github.com/filecoin-project/builtin-actors/releases/download/v11.0.0-rc2/builtin-actors-calibrationnet.car").unwrap()
    }),
    },
    HeightInfo {
        height: Height::Thunder,
        epoch: LIGHTNING_EPOCH + LIGHTNING_ROLLOVER_PERIOD,
        bundle: None,
    },
];
}

pub(super) static DRAND_SCHEDULE: [DrandPoint<'static>; 1] = [DrandPoint {
    height: 0,
    config: &DRAND_MAINNET,
}];
