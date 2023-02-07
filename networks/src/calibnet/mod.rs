// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{drand::DRAND_MAINNET, DrandPoint, Height, HeightInfo};

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

/// Height epochs.
pub const HEIGHT_INFOS: [HeightInfo; 18] = [
    HeightInfo {
        height: Height::Breeze,
        epoch: -1,
    },
    HeightInfo {
        height: Height::Smoke,
        epoch: -2,
    },
    HeightInfo {
        height: Height::Ignition,
        epoch: -3,
    },
    HeightInfo {
        height: Height::ActorsV2,
        epoch: 30,
    },
    HeightInfo {
        height: Height::Tape,
        epoch: 60,
    },
    HeightInfo {
        height: Height::Liftoff,
        epoch: -5,
    },
    HeightInfo {
        height: Height::Kumquat,
        epoch: 90,
    },
    HeightInfo {
        height: Height::Calico,
        epoch: 120,
    },
    HeightInfo {
        height: Height::Persian,
        epoch: 130,
    },
    HeightInfo {
        height: Height::Orange,
        epoch: 300,
    },
    HeightInfo {
        height: Height::Trust,
        epoch: 330,
    },
    HeightInfo {
        height: Height::Norwegian,
        epoch: 360,
    },
    HeightInfo {
        height: Height::Turbo,
        epoch: 390,
    },
    HeightInfo {
        height: Height::Hyperdrive,
        epoch: 420,
    },
    HeightInfo {
        height: Height::Chocolate,
        epoch: 450,
    },
    HeightInfo {
        height: Height::OhSnap,
        epoch: 480,
    },
    HeightInfo {
        height: Height::Skyr,
        epoch: 510,
    },
    HeightInfo {
        height: Height::Shark,
        epoch: 16_800,
    },
];

pub(super) static DRAND_SCHEDULE: [DrandPoint<'static>; 1] = [DrandPoint {
    height: 0,
    config: &DRAND_MAINNET,
}];
