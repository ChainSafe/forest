// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{drand::DRAND_MAINNET, DrandPoint};

/// Default genesis car file bytes.
pub const DEFAULT_GENESIS: &[u8] = include_bytes!("genesis.car");

/// Bootstrap peer ids
pub const DEFAULT_BOOTSTRAP: &[&str] = &[
    "/dns4/bootstrap-0.calibration.fildev.network/tcp/1347/p2p/12D3KooWJkikQQkxS58spo76BYzFt4fotaT5NpV2zngvrqm4u5ow",
    "/dns4/bootstrap-1.calibration.fildev.network/tcp/1347/p2p/12D3KooWLce5FDHR4EX4CrYavphA5xS3uDsX6aoowXh5tzDUxJav",
    "/dns4/bootstrap-2.calibration.fildev.network/tcp/1347/p2p/12D3KooWA9hFfQG9GjP6bHeuQQbMD3FDtZLdW1NayxKXUT26PQZu",
    "/dns4/bootstrap-3.calibration.fildev.network/tcp/1347/p2p/12D3KooWMHDi3LVTFG8Szqogt7RkNXvonbQYqSazxBx41A5aeuVz",
];

lazy_static! {
    pub(super) static ref DRAND_SCHEDULE: [DrandPoint<'static>; 1] = [DrandPoint {
        height: 0,
        config: &*DRAND_MAINNET,
    },];
}
