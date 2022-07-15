// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{drand::DRAND_MAINNET, DrandPoint, Height, HeightInfo};

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");

pub const MINIMUM_CONSENSUS_POWER: i64 = 32 << 30;

/// Bootstrap peer ids.
pub const DEFAULT_BOOTSTRAP: &[&str] = &[
    "/dns4/bootstrap-0.calibration.fildev.network/tcp/1347/p2p/12D3KooWJkikQQkxS58spo76BYzFt4fotaT5NpV2zngvrqm4u5ow",
    "/dns4/bootstrap-1.calibration.fildev.network/tcp/1347/p2p/12D3KooWLce5FDHR4EX4CrYavphA5xS3uDsX6aoowXh5tzDUxJav",
    "/dns4/bootstrap-2.calibration.fildev.network/tcp/1347/p2p/12D3KooWA9hFfQG9GjP6bHeuQQbMD3FDtZLdW1NayxKXUT26PQZu",
    "/dns4/bootstrap-3.calibration.fildev.network/tcp/1347/p2p/12D3KooWMHDi3LVTFG8Szqogt7RkNXvonbQYqSazxBx41A5aeuVz",
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
        height: Height::Claus,
        epoch: 270,
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
        epoch: 312_746,
    },
    HeightInfo {
        height: Height::OhSnap,
        epoch: 682_006,
    },
    HeightInfo {
        height: Height::Skyr,
        epoch: 1_044_660,
    },
];

lazy_static! {
    pub(super) static ref DRAND_SCHEDULE: [DrandPoint<'static>; 1] = [DrandPoint {
        height: 0,
        config: &DRAND_MAINNET,
    },];
}
